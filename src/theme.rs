//! Terminal colors from fuxx's settings: the `Theme` struct, color parsing, and theme
//! file resolution.
//!
//! Pure: no filesystem access here. `resolve` takes a `read_theme` closure and `user_theme_dir`
//! only builds a path. The rules come from Ghostty 1.3.1: theme files are plain config files (their
//! `theme` and `config-file` keys are ignored), the user's own values override the theme's, an
//! empty value resets a key, and `palette = N=COLOR` takes a decimal or `0x`/`0o`/`0b` index.

use crate::settings::{Entry, parse};
use crate::tab_color::TabColor;
use crate::ui_tokens::{MARK_MIN, QUIET_LINE_MIN, TEXT_MIN, contrast, ensure_contrast};
use alacritty_terminal::vte::ansi::Rgb;
use std::cell::Cell;
use std::path::{Path, PathBuf};

const fn hex(v: u32) -> Rgb {
    Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    }
}

const DEFAULT_BACKGROUND: Rgb = hex(0x282c34);
const DEFAULT_FOREGROUND: Rgb = hex(0xffffff);
const DEFAULT_PALETTE: [Rgb; 16] = [
    hex(0x1d1f21),
    hex(0xcc6666),
    hex(0xb5bd68),
    hex(0xf0c674),
    hex(0x81a2be),
    hex(0xb294bb),
    hex(0x8abeb7),
    hex(0xc5c8c6),
    hex(0x666666),
    hex(0xd54e53),
    hex(0xb9ca4a),
    hex(0xe7c547),
    hex(0x7aa6da),
    hex(0xc397d8),
    hex(0x70c0b1),
    hex(0xeaeaea),
];

/// The colors fuxx draws the terminal with. `None` means "derive it from the other colors".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub background: Rgb,
    pub foreground: Rgb,
    pub cursor: Option<Rgb>,
    pub cursor_text: Option<Rgb>,
    pub selection_background: Option<Rgb>,
    pub selection_foreground: Option<Rgb>,
    pub palette: [Rgb; 16],
    /// The theme's own window-chrome colours (`chrome-*` keys), where it sets them.
    pub chrome: Chrome,
}

/// A theme's own chrome colours (chore 2026-11-20, your call: "give each theme its own chrome
/// colours"): each one set replaces what `ui()` would derive for it from the terminal's colours,
/// still held to the same contrast floors. The built-in themes set all five (`builtin_theme`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chrome {
    /// The accent (selection, the shown tab, focus); derived from palette 4 (blue).
    pub accent: Option<Rgb>,
    /// The window's canvas, the sidebar's colour; derived by darkening (lightly inking) the
    /// background.
    pub canvas: Option<Rgb>,
    /// The status colours: working (palette 3), needs you (palette 1), done (palette 2).
    pub working: Option<Rgb>,
    pub waiting: Option<Rgb>,
    pub done: Option<Rgb>,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            background: DEFAULT_BACKGROUND,
            foreground: DEFAULT_FOREGROUND,
            cursor: None,
            cursor_text: None,
            selection_background: None,
            selection_foreground: None,
            palette: DEFAULT_PALETTE,
            chrome: Chrome::default(),
        }
    }
}

/// Linear blend per channel: `t = 0` gives `a`, `t = 1` gives `b`.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb {
        r: ch(a.r, b.r),
        g: ch(a.g, b.g),
        b: ch(a.b, b.b),
    }
}

/// Perceived brightness on 0-1, from the simple `0.2126R + 0.7152G + 0.0722B` weights applied to
/// the gamma-encoded channels (no sRGB linearisation: only used to call a color dark or light).
fn luminance(c: Rgb) -> f32 {
    (0.2126 * c.r as f32 + 0.7152 * c.g as f32 + 0.0722 * c.b as f32) / 255.0
}

/// The window chrome (sidebar and status rings), derived from the terminal theme so the whole
/// window matches it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiColors {
    pub sidebar_background: Rgb,
    pub sidebar_border: Rgb,
    pub row_active: Rgb,
    pub row_hover: Rgb,
    pub text: Rgb,
    pub text_dim: Rgb,
    pub ring_idle: Rgb,
    pub ring_working: Rgb,
    pub ring_waiting: Rgb,
    pub ring_done: Rgb,
    /// The one accent: the selected row's fill, focus, drop lines, a hovered divider.
    pub accent: Rgb,
    /// Text and marks on the accent (the selected sidebar row's until M11i): `on_accent`.
    pub on_accent: Rgb,
    /// Meta text (numbers, times, the waiting question): quieter than `text_dim`, still readable.
    pub text_faint: Rgb,
    /// A plain 1 px line inside panels and settings (it split panes until M11i; since M11al
    /// panes in the one card are split by a `card_border` hairline).
    pub divider: Rgb,
    /// A waiting row's fill; `done_tint` is the same, subtler, for a finished one.
    pub waiting_tint: Rgb,
    pub done_tint: Rgb,
    /// A hovered waiting row's fill, and a hovered row of an overlay (palette, menu).
    pub waiting_hover: Rgb,
    pub panel_hover: Rgb,
    pub focus_ring: Rgb,
    /// Overlays (palette, popups): a surface raised above the window, its edge and its shadow.
    pub panel: Rgb,
    pub panel_border: Rgb,
    pub shadow: Rgb,
    /// Selected text in a box (the rename, filter and address boxes): the accent washed over
    /// the box, as strong as keeps the text readable.
    pub selection: Rgb,
    /// The tab colours (`TabColor::ALL` order), fitted to stand out on every surface as marks do.
    pub tab_colors: [Rgb; 11],
    /// A raised card (each pane, the right sidebar; M11i): the terminal's own background.
    pub card: Rgb,
    /// A card's 1 px edge, quiet on the canvas (the sidebar colour) and on the card.
    pub card_border: Rgb,
    /// The Changes progress bar's unfilled track, on the repo card (`card_hover`; M11al Task 9).
    pub progress_track: Rgb,
    /// The card's shadow: `shadow` at this opacity (0-1).
    pub card_shadow_alpha: f32,
    /// M11al: the sidebar search box's fill, halfway from the canvas to the card.
    pub raised: Rgb,
    /// The right panel's surface inside the card (between the canvas and the card), its hovered
    /// tint and its selected tint (the `row_selected` formula on it): the Files, Changes and
    /// Graph rows' fills since M11al.
    pub side: Rgb,
    pub side_hover: Rgb,
    pub side_selected: Rgb,
    /// The active tab's fill: a faint wash of the accent over the card.
    pub tab_active: Rgb,
    /// The selected sidebar row's soft tint of the accent (its text keeps its own colours).
    pub row_selected: Rgb,
    /// A hovered and a selected fill on the card (tab hover, buttons, the repo card; the file
    /// tab's Wrap button and view switch).
    pub card_hover: Rgb,
    pub card_selected: Rgb,
    /// The Files tree's 1 px indent guides, quiet on `side` and its hover and selection.
    pub guide: Rgb,
    /// Command blocks' gutter bars (M11l), held to `MARK_MIN` on the terminal: a finished
    /// command (exit 0 or not reported), a failed one, a running one.
    pub block_ok: Rgb,
    pub block_failed: Rgb,
    pub block_running: Rgb,
    /// The selected sidebar row in a group's card (M11r): a neutral lift of the card.
    pub card_lift: Rgb,
    /// Under a dragged command block (M11aa Task 11): a pane tab that takes it (an agent's),
    /// and an agent tab's page. The accent washed over the bar and the card, as strong as
    /// keeps their text (and the tab's marks) readable.
    pub drop_tab: Rgb,
    pub drop_card: Rgb,
}

impl UiColors {
    /// Tab colour `c` in this theme.
    pub fn tab(&self, c: TabColor) -> Rgb {
        self.tab_colors[c as usize]
    }
}

/// What reads on the accent `accent` (the selected sidebar row's fill): of the theme's own
/// `background` and `foreground`, the one that contrasts more with it, pushed (hue kept) to
/// `TEXT_MIN` if it falls short.
pub fn on_accent(accent: Rgb, background: Rgb, foreground: Rgb) -> Rgb {
    let ink = if contrast(background, accent) >= contrast(foreground, accent) {
        background
    } else {
        foreground
    };
    ensure_contrast(ink, accent, TEXT_MIN)
}

/// `fg` moved along its hue (see `ensure_contrast`) until it reaches `min` on every one of `bgs`.
/// The chrome's surfaces all sit on the same side of a foreground, so moving away from one never
/// loses another; the repeat is a guard for odd user themes where they don't.
pub(crate) fn readable(mut fg: Rgb, bgs: &[Rgb], min: f32) -> Rgb {
    for _ in 0..3 {
        let before = fg;
        for &bg in bgs {
            fg = ensure_contrast(fg, bg, min);
        }
        if fg == before {
            break;
        }
    }
    fg
}

thread_local! {
    /// The last theme `ui()` saw and its chrome colours.
    static UI_CACHE: Cell<Option<(Theme, UiColors)>> = const { Cell::new(None) };
    #[cfg(test)]
    static UI_COMPUTATIONS: Cell<usize> = const { Cell::new(0) };
}

/// How many times `ui_uncached` has run on this thread.
#[cfg(test)]
fn ui_computations() -> usize {
    UI_COMPUTATIONS.with(Cell::get)
}

impl Theme {
    /// Whether the background is dark (the chrome, and the browser windows, go dark with it).
    pub fn is_dark(&self) -> bool {
        luminance(self.background) < 0.5
    }

    /// The chrome colours. Surfaces are mixed from the theme; everything drawn on them is then
    /// pushed (hue kept) to the contrast floors in `ui_tokens`, so light themes read as well as
    /// dark ones.
    pub fn ui(&self) -> UiColors {
        // The contrast math is about 8 % of a frame and `ui()` runs many times per render, so the
        // last theme's result is kept. The key is the whole `Theme` value, not a name, so a switch,
        // a light/dark change, a reload or a live edit of one colour all recompute.
        UI_CACHE.with(|cache| {
            if let Some((theme, ui)) = cache.get()
                && theme == *self
            {
                return ui;
            }
            let ui = self.ui_uncached();
            cache.set(Some((*self, ui)));
            ui
        })
    }

    fn ui_uncached(&self) -> UiColors {
        #[cfg(test)]
        UI_COMPUTATIONS.with(|n| n.set(n.get() + 1));
        let (bg, fg) = (self.background, self.foreground);
        let black = hex(0x000000);
        let white = hex(0xffffff);
        let dark = self.is_dark();
        // The sidebar colour is the window's canvas (M11al): the darkest surface. A dark one sinks
        // well below the terminal; a light one dips toward the theme's own ink (black would turn
        // it grey).
        let chrome = self.chrome;
        let canvas = chrome.canvas.unwrap_or(if dark {
            mix(bg, black, 0.42)
        } else {
            mix(bg, fg, 0.08)
        });
        // The sidebar colour before M11al. Text and selections keep mixing from it, and the
        // surfaces that were derived from it (rows, tints) are also kept as
        // "legacy" floors for text and marks, so the text hierarchy does not drift while the
        // canvas darkens (pinned by `dim_and_faint_text_keep_todays_values`).
        let base = if dark {
            mix(bg, black, 0.18)
        } else {
            mix(bg, fg, 0.04)
        };
        // M11al's floating card holds the panes; the attached sidebar sits between it and the
        // canvas; the sidebar's search box a half step from the canvas to the card.
        let card = bg;
        let card_lift = mix(card, fg, if dark { 0.10 } else { 0.07 });
        let raised = mix(canvas, card, 0.5);
        let side = mix(card, canvas, 0.33);
        let side_hover = mix(side, fg, 0.06);
        // Overlays are raised: a little lighter than the terminal on dark, near white on light.
        let panel = if dark {
            mix(bg, fg, 0.06)
        } else {
            mix(bg, white, 0.65)
        };
        let panel_hover = mix(panel, fg, 0.08);
        // Rows are mixed from the sidebar they sit on, toward the ink: hover a little, active more.
        let rows_on = |on: Rgb| {
            (
                mix(on, fg, 0.06),
                mix(on, fg, if dark { 0.14 } else { 0.10 }),
            )
        };
        let (row_hover, row_active) = rows_on(canvas);
        // Nothing draws these since M11al Task 8 (no group cards, rows on the canvas); kept as
        // floors because without them Mocha's `text_faint` moves a step (9fa2b3 → 9fa2b2).
        let (legacy_hover, legacy_active) = rows_on(base);
        let plain = [
            base,
            legacy_hover,
            legacy_active,
            panel,
            panel_hover,
            bg,
            canvas,
            row_hover,
            row_active,
        ];
        let waiting = readable(chrome.waiting.unwrap_or(self.palette[1]), &plain, MARK_MIN);
        let done = readable(chrome.done.unwrap_or(self.palette[2]), &plain, MARK_MIN);
        // The card is the terminal; a hovered row on it (and, M11al, a hovered pane tab).
        let card_hover = mix(card, fg, 0.06);
        // What a sidebar colour `on` carries: a soft wash of the status colour (done is subtler:
        // it needs you less).
        let family = |on: Rgb| {
            let waiting_tint = mix(on, waiting, if dark { 0.12 } else { 0.08 });
            let done_tint = mix(on, done, if dark { 0.07 } else { 0.05 });
            let waiting_hover = mix(waiting_tint, fg, 0.06);
            [waiting_tint, done_tint, waiting_hover]
        };
        // Tints stay on the pre-M11al colour (the tinted rows are redrawn on `side`): from the
        // darker canvas they would press the text of a light theme together.
        let [waiting_tint, done_tint, waiting_hover] = family(base);
        // Every surface something is drawn on: rows, tinted rows, overlays, pane tabs. The
        // older ones come first, in their old order: `readable` moves a colour step by step, so
        // the order keeps the text's values where they were.
        let legacy_surfaces = [
            base,
            legacy_hover,
            legacy_active,
            panel,
            panel_hover,
            bg,
            waiting_tint,
            done_tint,
            waiting_hover,
        ];
        // The rows on the canvas, and M11al's: the canvas, the search box, the attached
        // sidebar and its hover, the lift.
        let fresh_surfaces = [
            row_hover, row_active, canvas, raised, side, side_hover, card_lift,
        ];
        let surfaces = [&legacy_surfaces[..], &fresh_surfaces[..]].concat();
        let mark = |c: Rgb| readable(c, &surfaces, MARK_MIN);
        let accent = mark(chrome.accent.unwrap_or(self.palette[4]));
        let ring_working = mark(chrome.working.unwrap_or(self.palette[3]));
        // A selection is a soft wash of the accent over its surface, lowered in 2 % steps while
        // the text (as it reads on the other surfaces) would fall below 4.5:1 on it. At 0 it is
        // the surface itself, which the text reads on.
        let plain_text = readable(
            fg,
            &[&legacy_surfaces[..], &[card_hover]].concat(),
            TEXT_MIN,
        );
        // 18 % on dark themes, 12 % on light ones, in 2 % steps.
        let steps: u8 = if dark { 9 } else { 6 };
        let tint = |on: Rgb| {
            (1..=steps)
                .rev()
                .map(|n| mix(on, accent, 0.02 * n as f32))
                .find(|&s| contrast(plain_text, s) >= TEXT_MIN)
                .unwrap_or(on)
        };
        let row_selected = tint(canvas);
        let legacy_selected = tint(base);
        let card_selected = tint(card);
        let side_selected = tint(side);
        // The status marks read on the selections too: a theme's own accent (2026-11-20) can
        // make them darker than the blue one did (Latte's peach and green, on its mauve).
        let selections = [row_selected, legacy_selected, card_selected, side_selected];
        let ring_working = readable(ring_working, &selections, MARK_MIN);
        // Text reads on every surface, the new ones too.
        let text_surfaces = [
            &legacy_surfaces[..],
            &[card_hover, legacy_selected, card_selected, card_lift],
            &fresh_surfaces[..6],
            &[row_selected, side_selected],
        ]
        .concat();
        let text = |c: Rgb| readable(c, &text_surfaces, TEXT_MIN);
        let text_dim = text(mix(base, fg, 0.72));
        let quiet = |c: Rgb, on: &[Rgb]| readable(c, on, QUIET_LINE_MIN);
        let ink = text(fg);
        // The accent over the box (drawn on the sidebar colour), 30 % or less: as much as keeps
        // the text on it readable. At 0 it is the box itself, which the text reads on.
        let selection = (0..=6)
            .map(|step| mix(base, accent, 0.30 - 0.05 * step as f32))
            .find(|&s| contrast(ink, s) >= TEXT_MIN)
            .unwrap_or(base);
        let text_faint = text(mix(base, fg, 0.55));
        // Also text: the browser panel's error on the sidebar, and a waiting row's "Needs
        // input" on its tint (hovered or not) or, selected, on the selection's (M11i).
        let ring_waiting = readable(
            mark(waiting),
            &[
                canvas,
                base,
                waiting_tint,
                waiting_hover,
                row_selected,
                side_selected,
                legacy_selected,
            ],
            TEXT_MIN,
        );
        let ring_done = readable(mark(done), &selections, MARK_MIN);
        // Under a dragged command block (M11aa Task 11): a pane tab, and an agent tab's page
        // (the card). A wash of the accent (20 %; 8 % on the page), lowered in 2 % steps while
        // the text (and, on a tab, its status marks) would fall below its floor. At 0 it is
        // the surface itself, which they read on.
        let wash = |on: Rgb, most: f32, keep: &[(Rgb, f32)]| {
            (0..=10)
                .map(|n| mix(on, accent, (most - 0.02 * n as f32).max(0.0)))
                .find(|&s| keep.iter().all(|&(c, min)| contrast(c, s) >= min))
                .unwrap_or(on)
        };
        let words = [
            (ink, TEXT_MIN),
            (text_dim, TEXT_MIN),
            (text_faint, TEXT_MIN),
        ];
        let tab_marks = [
            (text_dim, MARK_MIN), // ring_idle
            (ring_working, MARK_MIN),
            (ring_waiting, MARK_MIN),
            (ring_done, MARK_MIN),
            (accent, MARK_MIN),
        ];
        // The active tab: the accent over the card at 10 % (8 % on light), lowered in 2 % steps
        // while the words or the tab's marks would fall below their floors. At 0 it is the card.
        let tab_active = wash(
            card,
            if dark { 0.10 } else { 0.08 },
            &[&words[..], &tab_marks[..]].concat(),
        );
        // The pane tab bars have no fill (M11al): a tab under a dragged block is washed over
        // the card.
        let drop_tab = wash(card, 0.20, &[&words[..], &tab_marks[..]].concat());
        let drop_card = wash(card, 0.08, &words);
        let tab_colors = TabColor::ALL.map(|c| readable(mark(c.base()), &selections, MARK_MIN));
        UiColors {
            sidebar_background: canvas,
            sidebar_border: quiet(mix(canvas, fg, 0.12), &[canvas, bg]),
            row_active,
            row_hover,
            text: ink,
            text_dim,
            ring_idle: text_dim,
            ring_working,
            ring_waiting,
            ring_done,
            accent,
            on_accent: on_accent(accent, bg, fg),
            text_faint,
            divider: quiet(mix(bg, fg, 0.16), &[bg]),
            waiting_tint,
            done_tint,
            waiting_hover,
            panel_hover,
            focus_ring: accent,
            panel,
            panel_border: quiet(mix(panel, fg, 0.16), &[panel, bg]),
            // Overlays' shadow, drawn with transparency: black on dark, the ink darkened on light.
            shadow: if dark { black } else { mix(fg, black, 0.5) },
            selection,
            tab_colors,
            card,
            card_border: quiet(
                mix(bg, fg, if dark { 0.10 } else { 0.12 }),
                &[canvas, card, side],
            ),
            progress_track: quiet(mix(card_hover, fg, 0.12), &[card_hover]),
            card_shadow_alpha: if dark { 0.45 } else { 0.12 },
            raised,
            side,
            side_hover,
            side_selected,
            tab_active,
            row_selected,
            card_hover,
            card_selected,
            guide: quiet(mix(side, fg, 0.12), &[side, side_hover, side_selected]),
            block_ok: ensure_contrast(self.palette[8], bg, MARK_MIN),
            block_failed: ensure_contrast(self.palette[1], bg, MARK_MIN),
            block_running: ensure_contrast(ring_working, bg, MARK_MIN),
            card_lift,
            drop_tab,
            drop_card,
        }
    }
}

/// A foreground drawn on a background, and the contrast it must reach.
#[cfg(any(test, feature = "ui-shots"))]
#[derive(Clone, Copy, Debug)]
pub struct DrawnPair {
    pub fg_name: &'static str,
    pub fg: Rgb,
    pub bg_name: &'static str,
    pub bg: Rgb,
    pub min: f32,
}

#[cfg(any(test, feature = "ui-shots"))]
impl Theme {
    /// Every chrome colour pair the UI draws, with its floor: what the contrast test checks and
    /// the token swatch board shows. A UI change that draws a token on a new surface adds it here.
    pub fn drawn_pairs(&self) -> Vec<DrawnPair> {
        let ui = self.ui();
        // Rows, tinted rows, overlays and pane tab bars (on the sidebar colour).
        let surfaces = [
            ("sidebar", ui.sidebar_background),
            ("row_hover", ui.row_hover),
            ("row_active", ui.row_active),
            ("waiting_tint", ui.waiting_tint),
            ("done_tint", ui.done_tint),
            ("waiting_hover", ui.waiting_hover),
            ("panel", ui.panel),
            ("panel_hover", ui.panel_hover),
            ("terminal", self.background),
            // M11al: the search box, the attached sidebar and its hover (marks read on these).
            ("raised", ui.raised),
            ("side", ui.side),
            ("side_hover", ui.side_hover),
        ];
        let text = [
            ("text", ui.text),
            ("text_dim", ui.text_dim),
            ("text_faint", ui.text_faint),
        ];
        let marks = [
            ("ring_idle", ui.ring_idle),
            ("ring_working", ui.ring_working),
            ("ring_waiting", ui.ring_waiting),
            ("ring_done", ui.ring_done),
            ("accent", ui.accent),
            ("focus_ring", ui.focus_ring),
        ];
        let pair = |(fg_name, fg): (&'static str, Rgb), (bg_name, bg): (&'static str, Rgb), min| {
            DrawnPair {
                fg_name,
                fg,
                bg_name,
                bg,
                min,
            }
        };
        // Tab colours are drawn as dots wherever a mark is (rows, palette, menu).
        let tabs: Vec<(&'static str, Rgb)> = TabColor::ALL.map(|c| (c.name(), ui.tab(c))).into();
        // M11i's surfaces. The selected sidebar row keeps its marks and tab dot in their own
        // colours, so they are checked there too.
        let row_selected = ("row_selected", ui.row_selected);
        let side_selected = ("side_selected", ui.side_selected);
        // Text-only here: no status mark or tab colour is drawn on these. `card_selected` is the
        // file tab's Wrap button and view switch (the Files tree's selected row moved to
        // `side_selected` in M11al). Tab colours must stay off it: Mocha's purple is 2.94:1 there.
        let text_only = [
            ("card_selected", ui.card_selected),
            // An agent tab's page under a dragged command block (M11aa Task 11).
            ("drop_card", ui.drop_card),
        ];
        // The pane tab bars (M11al: no fill, the card shows through): their titles, glyphs and
        // buttons, and a busy tab's status dot, on the card ("terminal", above), a hovered tab
        // or button (`card_hover`), the icon tile
        // (`card_lift`), the shown tab (`tab_active`), and a tab under a dragged command block
        // (`drop_tab`, M11aa Task 11); no tab colours.
        let tab_bars = [
            ("card_hover", ui.card_hover),
            ("card_lift", ui.card_lift),
            ("drop_tab", ui.drop_tab),
            ("tab_active", ui.tab_active),
        ];
        let marked: Vec<_> = surfaces
            .iter()
            .copied()
            .chain([row_selected, side_selected])
            .collect();
        let mark_room: Vec<_> = marked.iter().copied().chain(tab_bars).collect();
        let written: Vec<_> = mark_room.iter().copied().chain(text_only).collect();
        let mut pairs = vec![];
        for (fgs, min, on) in [
            (&text[..], TEXT_MIN, &written),
            (&marks[..], MARK_MIN, &mark_room),
            (&tabs[..], MARK_MIN, &marked),
        ] {
            for &fg in fgs {
                for &bg in on {
                    pairs.push(pair(fg, bg, min));
                }
            }
        }
        // Text and marks on the accent (the selected sidebar row until M11i gave it a tint).
        pairs.push(pair(
            ("on_accent", ui.on_accent),
            ("accent", ui.accent),
            TEXT_MIN,
        ));
        // Selected text in a box.
        pairs.push(pair(
            ("text", ui.text),
            ("selection", ui.selection),
            TEXT_MIN,
        ));
        // The browser panel prints its error in the waiting colour, as text.
        pairs.push(pair(
            ("ring_waiting", ui.ring_waiting),
            surfaces[0],
            TEXT_MIN,
        ));
        // A waiting sidebar row's "Needs input" (M11i), as text on its tint, hovered or not,
        // and on the selection's tint when it's selected.
        for on in [
            ("waiting_tint", ui.waiting_tint),
            ("waiting_hover", ui.waiting_hover),
            row_selected,
            side_selected,
        ] {
            pairs.push(pair(("ring_waiting", ui.ring_waiting), on, TEXT_MIN));
        }
        let terminal = ("terminal", self.background);
        let lines = [
            // Also the line between pane tabs (M11b), drawn on the bar's colour only.
            (("sidebar_border", ui.sidebar_border), surfaces[0]),
            (("sidebar_border", ui.sidebar_border), terminal),
            (("divider", ui.divider), terminal),
            (("panel_border", ui.panel_border), ("panel", ui.panel)),
            (("panel_border", ui.panel_border), terminal),
            // A card's edge on the canvas and on the card; the Files tree's guides on the panel's
            // fills (M11al Task 10).
            (("card_border", ui.card_border), surfaces[0]),
            (("card_border", ui.card_border), ("card", ui.card)),
            (("card_border", ui.card_border), ("side", ui.side)),
            // The sidebar's search box's edge on its own fill (M11al Task 7).
            (("card_border", ui.card_border), ("raised", ui.raised)),
            (("guide", ui.guide), ("side", ui.side)),
            (("guide", ui.guide), ("side_hover", ui.side_hover)),
            (("guide", ui.guide), ("side_selected", ui.side_selected)),
            // The Changes progress bar's unfilled track on the repo card (M11al Task 9).
            (
                ("progress_track", ui.progress_track),
                ("card_hover", ui.card_hover),
            ),
        ];
        for (line, bg) in lines {
            pairs.push(pair(line, bg, QUIET_LINE_MIN));
        }
        // A waiting row's Open button: the sidebar's colour on the waiting colour (M11r).
        pairs.push(pair(
            ("sidebar_background", ui.sidebar_background),
            ("ring_waiting", ui.ring_waiting),
            TEXT_MIN,
        ));
        pairs
    }
}

/// `#rrggbb` or `rrggbb`, any case. Everything else (names, 3-digit hex) is `None`.
pub fn parse_color(s: &str) -> Option<Rgb> {
    let s = s.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(s, 16).ok().map(hex)
}

/// A palette index: decimal, or with a `0x`/`0o`/`0b` prefix.
fn parse_index(s: &str) -> Option<usize> {
    let s = s.trim();
    let (digits, radix) = if let Some(r) = s.strip_prefix("0x") {
        (r, 16)
    } else if let Some(r) = s.strip_prefix("0o") {
        (r, 8)
    } else if let Some(r) = s.strip_prefix("0b") {
        (r, 2)
    } else {
        (s, 10)
    };
    usize::from_str_radix(digits, radix).ok()
}

fn skip(key: &str, value: &str) {
    eprintln!("[fuxx] config: ignoring bad value for `{key}`: {value:?}");
}

/// Set a required color: empty resets to `default`, a bad value is skipped.
fn set_color(slot: &mut Rgb, e: &Entry, default: Rgb) {
    if e.value.is_empty() {
        *slot = default;
    } else if let Some(c) = parse_color(&e.value) {
        *slot = c;
    } else {
        skip(&e.key, &e.value);
    }
}

/// Set an optional color: empty or `cell-*` clears it, a bad value is skipped.
fn set_optional(slot: &mut Option<Rgb>, e: &Entry) {
    if e.value.is_empty() || e.value == "cell-foreground" || e.value == "cell-background" {
        *slot = None;
    } else if let Some(c) = parse_color(&e.value) {
        *slot = Some(c);
    } else {
        skip(&e.key, &e.value);
    }
}

/// Apply the color keys of `entries` to `theme`, in order. Other keys are ignored.
pub fn apply(theme: &mut Theme, entries: &[Entry]) {
    for e in entries {
        match e.key.as_str() {
            "background" => set_color(&mut theme.background, e, DEFAULT_BACKGROUND),
            "foreground" => set_color(&mut theme.foreground, e, DEFAULT_FOREGROUND),
            "cursor-color" => set_optional(&mut theme.cursor, e),
            "cursor-text" => set_optional(&mut theme.cursor_text, e),
            "selection-background" => set_optional(&mut theme.selection_background, e),
            "selection-foreground" => set_optional(&mut theme.selection_foreground, e),
            "chrome-accent" => set_optional(&mut theme.chrome.accent, e),
            "chrome-canvas" => set_optional(&mut theme.chrome.canvas, e),
            "chrome-working" => set_optional(&mut theme.chrome.working, e),
            "chrome-waiting" => set_optional(&mut theme.chrome.waiting, e),
            "chrome-done" => set_optional(&mut theme.chrome.done, e),
            "palette" => {
                if e.value.is_empty() {
                    theme.palette = DEFAULT_PALETTE;
                    continue;
                }
                let parsed = e
                    .value
                    .split_once('=')
                    .and_then(|(n, c)| Some((parse_index(n)?, parse_color(c)?)));
                match parsed {
                    Some((n, c)) if n < 16 => theme.palette[n] = c,
                    Some(_) => {} // 16..=255 are not stored
                    None => skip(&e.key, &e.value),
                }
            }
            _ => {}
        }
    }
}

/// Pick the theme name from a `theme` value: plain, or `light:A,dark:B` in either order.
fn pick_theme_name(value: &str, dark: bool) -> Option<String> {
    let want = if dark { "dark" } else { "light" };
    if value.contains("light:") || value.contains("dark:") {
        value.split(',').find_map(|part| {
            let (k, v) = part.split_once(':')?;
            (k.trim() == want).then(|| v.trim().to_string())
        })
    } else {
        Some(value.trim().to_string())
    }
}

/// Defaults, then the named theme (if any), then the user's own entries on top.
pub fn resolve(
    entries: &[Entry],
    dark: bool,
    read_theme: &dyn Fn(&str) -> Option<String>,
) -> Theme {
    let mut theme = Theme::default();
    let name = entries
        .iter()
        .rev()
        .find(|e| e.key == "theme")
        .and_then(|e| pick_theme_name(&e.value, dark))
        .filter(|n| !n.is_empty());
    if let Some(name) = name {
        let path_like = name.contains('/') && !Path::new(&name).is_absolute();
        let text = if path_like || name == "." || name == ".." {
            None
        } else {
            read_theme(&name)
        };
        match text {
            Some(text) => {
                let inner: Vec<Entry> = parse(&text)
                    .into_iter()
                    .filter(|e| e.key != "theme" && e.key != "config-file")
                    .collect();
                apply(&mut theme, &inner);
            }
            None => eprintln!(
                "[fuxx] config: theme {name:?} not found, using defaults (built in: {})",
                BUILTIN_THEMES.join(", ")
            ),
        }
    }
    apply(&mut theme, entries);
    theme
}

/// The folder for the user's own theme files: `{XDG_CONFIG_HOME or ~/.config}/fuxx/themes`.
pub fn user_theme_dir(home: &Path, xdg_config_home: Option<&Path>) -> PathBuf {
    xdg_config_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".config"))
        .join("fuxx/themes")
}

/// The themes built into fuxx (from iTerm2-Color-Schemes, MIT; see `assets/themes/`).
pub const BUILTIN_THEMES: [&str; 6] = [
    "Catppuccin Mocha",
    "Catppuccin Latte",
    "Dracula",
    "Nord",
    "Gruvbox Dark",
    "TokyoNight",
];

/// The text of a built-in theme: the upstream file, then fuxx's own chrome colours for it
/// (`assets/themes/chrome/`, from the theme's official palette; 2026-11-20). The name match is
/// exact and case-sensitive.
pub fn builtin_theme(name: &str) -> Option<&'static str> {
    macro_rules! with_chrome {
        ($file:literal) => {
            concat!(
                include_str!(concat!("../assets/themes/", $file)),
                "\n",
                include_str!(concat!("../assets/themes/chrome/", $file)),
            )
        };
    }
    Some(match name {
        "Catppuccin Mocha" => with_chrome!("Catppuccin Mocha"),
        "Catppuccin Latte" => with_chrome!("Catppuccin Latte"),
        "Dracula" => with_chrome!("Dracula"),
        "Nord" => with_chrome!("Nord"),
        "Gruvbox Dark" => with_chrome!("Gruvbox Dark"),
        "TokyoNight" => with_chrome!("TokyoNight"),
        _ => return None,
    })
}

/// Every built-in theme, resolved as `theme = <name>` does, and the default theme (for the
/// contrast tests here and in the UI pieces that push colours on their own surfaces).
#[cfg(test)]
pub(crate) fn every_theme() -> Vec<(&'static str, Theme)> {
    let entries = |text: &str| crate::settings::parse(text);
    let mut all: Vec<_> = BUILTIN_THEMES
        .iter()
        .map(|&name| {
            let t = resolve(&entries(&format!("theme = {name}")), true, &|n| {
                builtin_theme(n).map(str::to_string)
            });
            (name, t)
        })
        .collect();
    all.push(("default", Theme::default()));
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Entry;
    use crate::ui_tokens::{contrast, hue};
    use std::path::{Path, PathBuf};

    const MOCHA: &str = "palette = 1=#f38ba8\nbackground = #1e1e2e\nforeground = #cdd6f4\ncursor-color = #f5e0dc\nselection-background = #585b70\ntheme = Nested\nconfig-file = evil";
    fn entries(text: &str) -> Vec<Entry> {
        crate::settings::parse(text)
    }
    fn read(name: &str) -> Option<String> {
        (name == "Catppuccin Mocha").then(|| MOCHA.to_string())
    }

    /// Chore 2026-11-20 (your call: "give each theme its own chrome colours"): a theme sets its
    /// chrome's accent, canvas and status colours with `chrome-*` keys; empty clears one.
    #[test]
    fn a_theme_can_set_its_own_chrome_colours() {
        let mut t = Theme::default();
        apply(
            &mut t,
            &entries(
                "chrome-accent = #cba6f7\nchrome-canvas = #11111b\nchrome-working = #fab387\n\
                 chrome-waiting = #f38ba8\nchrome-done = #a6e3a1",
            ),
        );
        assert_eq!(
            t.chrome,
            Chrome {
                accent: Some(hex(0xcba6f7)),
                canvas: Some(hex(0x11111b)),
                working: Some(hex(0xfab387)),
                waiting: Some(hex(0xf38ba8)),
                done: Some(hex(0xa6e3a1)),
            }
        );
        apply(&mut t, &entries("chrome-accent =\nchrome-done = nope"));
        assert_eq!(t.chrome.accent, None, "empty clears it");
        assert_eq!(t.chrome.done, Some(hex(0xa6e3a1)), "a bad value is skipped");
    }

    /// The chrome's own colours replace the derived ones (held to the same floors: these read).
    #[test]
    fn a_themes_chrome_colours_replace_the_derived_ones() {
        let mut t = Theme {
            background: hex(0x1e1e2e),
            foreground: hex(0xcdd6f4),
            ..Theme::default()
        };
        let derived = t.ui();
        t.chrome = Chrome {
            accent: Some(hex(0xcba6f7)),
            canvas: Some(hex(0x11111b)),
            working: Some(hex(0xfab387)),
            waiting: Some(hex(0xf38ba8)),
            done: Some(hex(0xa6e3a1)),
        };
        let ui = t.ui();
        assert_eq!(ui.sidebar_background, hex(0x11111b));
        assert_eq!(ui.accent, hex(0xcba6f7));
        assert_eq!(ui.ring_working, hex(0xfab387));
        assert_eq!(ui.ring_waiting, hex(0xf38ba8));
        assert_eq!(ui.ring_done, hex(0xa6e3a1));
        assert_ne!(ui.accent, derived.accent, "not the theme's blue");
    }

    /// Every built-in theme has its own chrome (2026-11-20), and its accent stands apart from
    /// its status colours (one accent; colour otherwise means status).
    #[test]
    fn every_builtin_theme_has_its_own_chrome() {
        for (name, theme) in every_theme() {
            if name == "default" {
                continue;
            }
            let c = theme.chrome;
            let all = [c.accent, c.canvas, c.working, c.waiting, c.done];
            assert!(all.iter().all(Option::is_some), "{name}: {c:?}");
            let ui = theme.ui();
            for (what, status) in [
                ("working", ui.ring_working),
                ("waiting", ui.ring_waiting),
                ("done", ui.ring_done),
            ] {
                let d = (hue(ui.accent) - hue(status)).abs();
                let d = d.min(360.0 - d);
                assert!(d >= 25.0, "{name}: the accent {d}° from {what}");
            }
        }
    }

    #[test]
    fn ui_is_computed_once_per_theme() {
        let t = Theme::default();
        let t2 = Theme {
            background: hex(0xfdf6e3),
            foreground: hex(0x657b83),
            ..Theme::default()
        };
        // Whatever an earlier test on this thread left in the cache, start from a theme
        // neither `t` nor `t2` is, so the counts below don't depend on test order.
        Theme {
            background: hex(0x010203),
            ..Theme::default()
        }
        .ui();
        let base = ui_computations();
        let a = t.ui();
        assert_eq!(t.ui(), a);
        assert_eq!(ui_computations() - base, 1);
        let b = t2.ui();
        assert_eq!(ui_computations() - base, 2);
        assert_ne!(a, b);
        assert_eq!(t.ui(), a);
        assert_eq!(ui_computations() - base, 3);
        for (name, theme) in every_theme() {
            assert_eq!(theme.ui(), theme.ui_uncached(), "{name}");
            assert_eq!(theme.ui(), theme.ui_uncached(), "{name} (cached)");
        }
    }

    #[test]
    fn ui_follows_a_live_edit_of_one_colour() {
        let mut t = Theme::default();
        let before = t.ui();
        // Same name-less theme, one field changed: the cache must not hand back the old chrome.
        t.background = hex(0x203040);
        assert_ne!(t.ui(), before);
        assert_eq!(t.ui(), t.ui_uncached());
        let mut u = t;
        u.palette[4] = hex(0x123456);
        assert_eq!(u.ui(), u.ui_uncached());
        let mut v = t;
        v.cursor = Some(hex(0xff0000));
        assert_eq!(v.ui(), v.ui_uncached());
    }

    #[test]
    fn defaults_are_ghosttys() {
        let t = Theme::default();
        assert_eq!(
            (t.background, t.foreground),
            (
                parse_color("#282c34").unwrap(),
                parse_color("ffffff").unwrap()
            )
        );
        assert_eq!(t.palette[1], parse_color("cc6666").unwrap());
        assert_eq!(t.palette[15], parse_color("eaeaea").unwrap());
    }

    #[test]
    fn a_named_theme_loads_and_user_values_override_it() {
        let t = resolve(
            &entries("theme = Catppuccin Mocha\nbackground = #000000"),
            true,
            &read,
        );
        assert_eq!(t.background, parse_color("000000").unwrap()); // the user's own value wins
        assert_eq!(t.foreground, parse_color("cdd6f4").unwrap()); // the theme fills in the rest
        assert_eq!(t.palette[1], parse_color("f38ba8").unwrap());
        assert_eq!(t.cursor, parse_color("f5e0dc"));
    }

    #[test]
    fn light_dark_pair_picks_by_appearance() {
        let e = entries("theme = light:Nope,dark:Catppuccin Mocha");
        assert_eq!(
            resolve(&e, true, &read).background,
            parse_color("1e1e2e").unwrap()
        );
        assert_eq!(resolve(&e, false, &read), Theme::default()); // "Nope" missing → defaults
    }

    #[test]
    fn bad_values_are_skipped() {
        let t = resolve(
            &entries(
                "background = navy\nforeground = #12345\npalette = 300=#ffffff\npalette = x=#ffffff\npalette = 0x2=#010203\ncursor-color = cell-foreground",
            ),
            true,
            &read,
        );
        assert_eq!(
            (t.background, t.foreground),
            (Theme::default().background, Theme::default().foreground)
        );
        assert_eq!(t.palette[2], parse_color("010203").unwrap());
        assert_eq!(t.cursor, None);
    }

    #[test]
    fn empty_value_resets_and_missing_theme_keeps_defaults() {
        let t = resolve(
            &entries("background = #000000\nbackground =\ntheme = Nope"),
            true,
            &read,
        );
        assert_eq!(t, Theme::default());
    }

    #[test]
    fn user_theme_dir_is_fuxx_own() {
        assert_eq!(
            user_theme_dir(Path::new("/h"), None),
            PathBuf::from("/h/.config/fuxx/themes")
        );
        assert_eq!(
            user_theme_dir(Path::new("/h"), Some(Path::new("/x"))),
            PathBuf::from("/x/fuxx/themes")
        );
    }

    #[test]
    fn builtin_themes_load_and_change_colors() {
        for name in BUILTIN_THEMES {
            let text = builtin_theme(name).unwrap_or_else(|| panic!("{name} missing"));
            let t = resolve(&entries(&format!("theme = {name}")), true, &|n| {
                builtin_theme(n).map(str::to_string)
            });
            assert_ne!(t, Theme::default(), "{name} changed nothing");
            assert!(text.contains("palette = 1="), "{name} has a palette");
        }
        assert_eq!(builtin_theme("catppuccin mocha"), None); // exact names only
        assert_eq!(builtin_theme("Nope"), None);
    }

    #[test]
    fn every_builtin_theme_meets_the_floors() {
        let mut failures = vec![];
        for (name, theme) in every_theme() {
            for p in theme.drawn_pairs() {
                let ratio = contrast(p.fg, p.bg);
                if ratio < p.min {
                    failures.push(format!(
                        "{name}: {} on {} is {ratio:.2}, needs {}",
                        p.fg_name, p.bg_name, p.min
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn text_on_the_accent_reads_in_every_theme() {
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            let ratio = contrast(ui.on_accent, ui.accent);
            assert!(ratio >= TEXT_MIN, "{name}: on_accent is {ratio:.2}");
            // It is the drawn pair the contrast test and the swatch board know.
            assert!(
                theme
                    .drawn_pairs()
                    .iter()
                    .any(|p| p.fg_name == "on_accent" && p.bg_name == "accent")
            );
        }
        // The theme's own ink, whichever reads better: dark ink on a light accent (Mocha's
        // blue), the light background on a dark one.
        let (dark, light) = (hex(0x1e1e2e), hex(0xeff1f5));
        assert_eq!(on_accent(hex(0x89b4fa), dark, hex(0xcdd6f4)), dark);
        assert_eq!(on_accent(hex(0x1e40af), light, hex(0x4c4f69)), light);
        // Short of 4.5:1 (Latte's blue on its own background is 4.35): pushed, hue kept.
        let latte_blue = hex(0x1e66f5);
        let out = on_accent(latte_blue, light, hex(0x4c4f69));
        assert!(contrast(light, latte_blue) < TEXT_MIN);
        assert!(contrast(out, latte_blue) >= TEXT_MIN, "{out:?}");
        // A mid-grey accent: still readable one way or the other.
        let grey = hex(0x777777);
        assert!(contrast(on_accent(grey, grey, grey), grey) >= TEXT_MIN);
    }

    #[test]
    fn a_selection_shows_and_its_text_reads() {
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            // The boxes are drawn on the sidebar colour.
            let shows = contrast(ui.selection, ui.sidebar_background);
            assert!(
                shows >= 1.25,
                "{name}: the selection is only {shows:.2} from its box"
            );
            let reads = contrast(ui.text, ui.selection);
            assert!(
                reads >= TEXT_MIN,
                "{name}: text on the selection is {reads:.2}"
            );
        }
    }

    #[test]
    fn new_surfaces_keep_text_readable_on_every_theme() {
        let mut failures = vec![];
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            let surfaces = [
                ("row_selected", ui.row_selected),
                ("card_selected", ui.card_selected),
                ("card_hover", ui.card_hover),
            ];
            for (token, fg) in [("text", ui.text), ("text_dim", ui.text_dim)] {
                for (surface, bg) in surfaces {
                    let ratio = contrast(fg, bg);
                    if ratio < TEXT_MIN {
                        failures.push(format!("{name}: {token} on {surface} is {ratio:.2}"));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn card_edges_and_guides_are_quiet_but_visible() {
        let (_, mocha) = every_theme()
            .into_iter()
            .find(|(name, _)| *name == "Catppuccin Mocha")
            .expect("Mocha is built in");
        let ui = mocha.ui();
        for (what, ratio) in [
            (
                "card_border on the canvas",
                contrast(ui.card_border, ui.sidebar_background),
            ),
            (
                "card_border on the card",
                contrast(ui.card_border, mocha.background),
            ),
            // M11al Task 10: the tree's guides are drawn on the attached sidebar's fills.
            ("guide on side", contrast(ui.guide, ui.side)),
            ("guide on side_hover", contrast(ui.guide, ui.side_hover)),
            (
                "guide on side_selected",
                contrast(ui.guide, ui.side_selected),
            ),
        ] {
            assert!(ratio >= QUIET_LINE_MIN, "{what} is {ratio:.2}: invisible");
            assert!(ratio < MARK_MIN, "{what} is {ratio:.2}: too loud");
        }
        // The card is the terminal's own background.
        assert_eq!(ui.card, mocha.background);
    }

    #[test]
    fn selection_is_a_tint_of_the_accent() {
        let themes = every_theme();
        let named = |name: &str| themes.iter().find(|(n, _)| *n == name).unwrap().1;
        let ui = named("Catppuccin Mocha").ui();
        assert_ne!(ui.row_selected, ui.sidebar_background);
        let d = (hue(ui.row_selected) - hue(ui.accent)).abs();
        assert!(
            d.min(360.0 - d) <= 15.0,
            "row_selected {:?} is {d:.1}° from the accent {:?}",
            ui.row_selected,
            ui.accent
        );
        assert_eq!(ui.card_shadow_alpha, 0.45);
        assert_eq!(named("Catppuccin Latte").ui().card_shadow_alpha, 0.12);
    }

    #[test]
    fn the_canvas_is_darkest_and_side_between() {
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            let l = |c| relative_luminance_for_test(c);
            assert!(
                l(ui.sidebar_background) < l(ui.side) && l(ui.side) < l(ui.card),
                "{name}: canvas {:?}, side {:?}, card {:?}",
                ui.sidebar_background,
                ui.side,
                ui.card
            );
        }
    }

    fn relative_luminance_for_test(c: Rgb) -> f32 {
        // contrast against black is monotonic in luminance
        contrast(c, hex(0x000000))
    }

    #[test]
    fn card_border_is_quiet_on_canvas_card_and_side() {
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            for (what, on) in [
                ("canvas", ui.sidebar_background),
                ("card", ui.card),
                ("side", ui.side),
            ] {
                let r = contrast(ui.card_border, on);
                assert!(
                    r >= QUIET_LINE_MIN,
                    "{name}: card_border on {what} is {r:.2}"
                );
            }
        }
    }

    #[test]
    fn text_reads_on_the_new_surfaces() {
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            for (sn, s) in [
                ("raised", ui.raised),
                ("side", ui.side),
                ("side_hover", ui.side_hover),
                ("side_selected", ui.side_selected),
                ("tab_active", ui.tab_active),
            ] {
                for (tn, t) in [
                    ("text", ui.text),
                    ("text_dim", ui.text_dim),
                    ("text_faint", ui.text_faint),
                ] {
                    let r = contrast(t, s);
                    assert!(r >= TEXT_MIN, "{name}: {tn} on {sn} is {r:.2}");
                }
            }
        }
    }

    #[test]
    fn marks_read_on_the_new_surfaces() {
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            for (sn, s) in [
                ("tab_active", ui.tab_active),
                ("side", ui.side),
                ("card_lift", ui.card_lift),
            ] {
                for (mn, m) in [
                    ("ring_working", ui.ring_working),
                    ("ring_waiting", ui.ring_waiting),
                    ("ring_done", ui.ring_done),
                    ("accent", ui.accent),
                ] {
                    let r = contrast(m, s);
                    assert!(r >= MARK_MIN, "{name}: {mn} on {sn} is {r:.2}");
                }
            }
        }
    }

    #[test]
    fn tab_active_keeps_the_accent_hue() {
        // The shown tab is the accent washed over the card (10 %, 8 % on light, or less in 2 %
        // steps): a tint of the accent, whatever hue the card leans to (a 10 % mauve wash on
        // Mocha's blue card reads 19° off mauve's own hue, 2026-11-20; it is still mauve's).
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            let washes: Vec<Rgb> = (0..=5)
                .map(|n| mix(ui.card, ui.accent, 0.10 - 0.02 * n as f32))
                .chain((0..=4).map(|n| mix(ui.card, ui.accent, 0.08 - 0.02 * n as f32)))
                .collect();
            assert!(
                washes.contains(&ui.tab_active),
                "{name}: tab_active {:?} isn't the accent {:?} over the card",
                ui.tab_active,
                ui.accent
            );
        }
        let (_, mocha) = every_theme()
            .into_iter()
            .find(|(n, _)| *n == "Catppuccin Mocha")
            .unwrap();
        assert_ne!(mocha.ui().tab_active, mocha.ui().card);
    }

    #[test]
    fn dim_and_faint_text_keep_todays_values() {
        // Read from master before M11al darkened the canvas (the text hierarchy must not drift);
        // moved on purpose 2026-11-20, when each theme got its own chrome: Mocha a step, Latte
        // three darker (its text must read on the mauve selection).
        let themes = every_theme();
        let named = |name: &str| themes.iter().find(|(n, _)| *n == name).unwrap().1.ui();
        let mocha = named("Catppuccin Mocha");
        assert_eq!(mocha.text_dim, hex(0x9da2bb));
        assert_eq!(mocha.text_faint, hex(0xa0a2b4));
        let latte = named("Catppuccin Latte");
        assert_eq!(latte.text_dim, hex(0x575769));
        assert_eq!(latte.text_faint, hex(0x585863));
    }

    #[test]
    fn drop_tab_reads_on_the_card() {
        // M11al: the pane tab bars have no fill (the card shows through), so a tab under a
        // dragged block is the accent washed over the card (20 % or less, in 2 % steps), with
        // the words and the tab's marks at their floors on it.
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            let washes: Vec<Rgb> = (0..=10)
                .map(|n| mix(ui.card, ui.accent, (0.20 - 0.02 * n as f32).max(0.0)))
                .collect();
            assert!(
                washes.contains(&ui.drop_tab),
                "{name}: drop_tab {:?} isn't the accent over the card {:?}",
                ui.drop_tab,
                ui.card
            );
            for (what, c, min) in [
                ("text", ui.text, TEXT_MIN),
                ("text_dim", ui.text_dim, TEXT_MIN),
                ("text_faint", ui.text_faint, TEXT_MIN),
                ("ring_working", ui.ring_working, MARK_MIN),
                ("ring_waiting", ui.ring_waiting, MARK_MIN),
                ("ring_done", ui.ring_done, MARK_MIN),
                ("accent", ui.accent, MARK_MIN),
            ] {
                let r = contrast(c, ui.drop_tab);
                assert!(r >= min, "{name}: {what} on drop_tab is {r:.2}");
            }
        }
    }

    #[test]
    fn a_dragged_block_tints_its_drop_targets_in_every_theme() {
        // M11aa Task 11: the tints show (not stepped all the way back to the surface) and keep
        // their text readable (`drawn_pairs` checks the floors).
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            assert_ne!(ui.drop_tab, ui.card, "{name}: drop_tab");
            assert_ne!(ui.drop_card, ui.card, "{name}: drop_card");
            assert!(contrast(ui.text_dim, ui.drop_tab) >= TEXT_MIN, "{name}");
            assert!(contrast(ui.text, ui.drop_card) >= TEXT_MIN, "{name}");
        }
    }

    #[test]
    fn light_themes_get_light_chrome() {
        let mut plain = Theme::default();
        plain.background = hex(0xffffff);
        plain.foreground = hex(0x000000);
        let (_, latte) = every_theme()
            .into_iter()
            .find(|(name, _)| *name == "Catppuccin Latte")
            .expect("Latte is built in");
        for (name, theme) in [("Catppuccin Latte", latte), ("plain light", plain)] {
            let ui = theme.ui();
            let l = luminance;
            assert!(
                l(ui.sidebar_background) > 0.5,
                "{name}: the sidebar is light"
            );
            // Hover and active darken the row (toward the ink), active more than hover.
            assert!(
                l(ui.row_hover) < l(ui.sidebar_background),
                "{name}: hover darkens"
            );
            assert!(
                l(ui.row_active) < l(ui.row_hover),
                "{name}: active is darker than hover"
            );
            // Overlays are raised: lighter than the sidebar, with a darker edge.
            assert!(
                l(ui.panel) > l(ui.sidebar_background),
                "{name}: the panel is raised"
            );
            assert!(
                l(ui.panel_border) < l(ui.panel),
                "{name}: the panel edge shows"
            );
            // Tints stay light; text stays dark.
            for (tint, c) in [("waiting", ui.waiting_tint), ("done", ui.done_tint)] {
                assert!(l(c) > 0.5, "{name}: the {tint} tint is light");
            }
            for (token, c) in [
                ("text", ui.text),
                ("dim", ui.text_dim),
                ("faint", ui.text_faint),
            ] {
                assert!(l(c) < 0.5, "{name}: {token} is dark ink");
            }
        }
    }

    #[test]
    fn dark_and_light_themes_are_told_apart() {
        let themes = every_theme();
        let named = |name: &str| themes.iter().find(|(n, _)| *n == name).unwrap().1;
        assert!(named("Catppuccin Mocha").is_dark());
        assert!(!named("Catppuccin Latte").is_dark());
        assert!(Theme::default().is_dark());
    }

    #[test]
    fn ui_colors_follow_light_and_dark_themes() {
        let dark = Theme::default();
        let ui = dark.ui();
        assert!(luminance(ui.sidebar_background) < luminance(dark.background));
        // Status colours keep their palette slot's hue; they may be lightened or darkened to read.
        for (ring, slot) in [
            (ui.ring_waiting, 1),
            (ui.ring_done, 2),
            (ui.ring_working, 3),
        ] {
            let d = (hue(ring) - hue(dark.palette[slot])).abs();
            assert!(
                d.min(360.0 - d) < 3.0,
                "ring for palette[{slot}] changed hue"
            );
        }

        let mut light = Theme::default();
        light.background = hex(0xffffff);
        light.foreground = hex(0x000000);
        let ui = light.ui();
        assert_ne!(ui.text_dim, light.background);
        let l = luminance(ui.row_active);
        assert!(l > luminance(light.foreground) && l < luminance(light.background));
    }
}
