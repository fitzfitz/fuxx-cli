//! The file tab's Markdown Preview (M11ac): a `.md` or `.markdown` file shown rendered, read-only,
//! instead of its source. The file is parsed with `agent_markdown::parse` (again only when the
//! tab reads a new version) and drawn as GPUI elements: a block per `Md`, each a row of a GPUI
//! `list` (only the rows in sight are drawn; its `ListState` is kept by the tab, so a reload keeps
//! the place, by block, as near as the new text allows).
//! Prose is in the UI font, wrapped to the tab's width; code (blocks and inline) in the
//! terminal's font on a tint of the text. Links are clickable: a web or mail link opens in the
//! default browser, one to a file next to it opens that file in a tab; anything else does
//! nothing. Colours come from the theme (`UiColors`), the text held to `TEXT_MIN`.
//!
//! `is_markdown`, `link_target`, `inline`, `marker` and `gap_between` are pure and tested.

use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, App, FontStyle, FontWeight, HighlightStyle, InteractiveText, RenderImage,
    SharedString, StyledText, UnderlineStyle, Window, div, img, prelude::*, px, relative,
};

use crate::agent_markdown::{Align, Md, Span};
use crate::file_tab::expand_tabs;
use crate::md_reader::{
    Badge, BlockLook, Callout, Front, ReaderPlan, RowKind, fence_lang, is_badge_src,
};
use crate::tab_bar::{Glyph, ICON, glyph};
use crate::terminal_view::hsla;
use crate::theme::{Theme, mix};
use crate::ui_tokens::reader::TABLE_COL_MIN;
use crate::ui_tokens::size::TERMINAL_PADDING;
use crate::ui_tokens::{MARK_MIN, QUIET_LINE_MIN, TEXT_MIN, ensure_contrast};

/// The sizes a Markdown block is drawn with (M11ai plan-time call 1): agent chat keeps today's
/// (`CHAT`), the Preview's reader has its own (`reader()`, from `ui_tokens::reader`). Line
/// heights are multiples of their text's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sizes {
    pub body: f32,
    pub line: f32,
    pub headings: [f32; 6],
    pub heading_line: f32,
    pub code: f32,
    pub code_line: f32,
    pub block_gap: f32,
    pub item_gap: f32,
    pub heading_above: f32,
    pub heading_below: f32,
    /// H1 / H2's underline, this far under the text.
    pub rule_below: f32,
    /// A code block's padding (sides, top and bottom) and corners.
    pub code_pad: (f32, f32),
    pub code_radius: f32,
    /// A list level's indent (and the marker's column), and a number's gap to its text.
    pub indent: f32,
    pub marker_gap: f32,
    /// A plain quote's padding left of its text, and its bar.
    pub quote_pad: f32,
    pub quote_bar: f32,
    /// A rule's margin above and below, and its line.
    pub rule_margin: f32,
    pub rule_line: f32,
    /// A table cell's padding (sides, top and bottom).
    pub table_cell: (f32, f32),
}

impl Sizes {
    /// M11ac's sizes, which agent chat keeps.
    pub const CHAT: Sizes = Sizes {
        body: 14.0,
        line: 1.5,
        headings: [24.0, 20.0, 17.0, 15.0, 14.0, 13.0],
        heading_line: 1.3,
        code: 13.0,
        code_line: 1.45,
        block_gap: 12.0,
        item_gap: 4.0,
        heading_above: 12.0,
        heading_below: 12.0,
        rule_below: 6.0,
        code_pad: (12.0, 10.0),
        code_radius: 6.0,
        indent: 20.0,
        marker_gap: 6.0,
        quote_pad: 12.0,
        quote_bar: 3.0,
        rule_margin: 4.0,
        rule_line: 1.0,
        table_cell: (10.0, 5.0),
    };

    /// The Preview's reader (M11ai spec A): bigger type, more room.
    pub fn reader() -> Sizes {
        use crate::ui_tokens::reader as r;
        Sizes {
            body: r::BODY,
            line: r::BODY_LINE,
            headings: r::HEADINGS,
            heading_line: r::HEADING_LINE,
            code: r::CODE,
            code_line: r::CODE_LINE,
            block_gap: r::BLOCK_GAP,
            item_gap: r::ITEM_GAP,
            heading_above: r::HEADING_ABOVE,
            heading_below: r::HEADING_BELOW,
            rule_below: r::RULE_BELOW,
            code_pad: r::CODE_PAD,
            code_radius: r::CODE_RADIUS,
            indent: r::LIST_INDENT,
            marker_gap: r::MARKER_GAP,
            quote_pad: r::QUOTE_PAD,
            quote_bar: r::QUOTE_BAR,
            rule_margin: r::RULE_MARGIN,
            rule_line: r::RULE_LINE,
            table_cell: r::TABLE_CELL,
        }
    }
}

/// Whether `path` names a Markdown file: `.md` or `.markdown`, in any case.
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// Where a link goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    /// `http://`, `https://` or `mailto:`: the default browser (or mail app).
    Web(String),
    /// A path, from the file's own folder unless absolute, and its `#part` (none when empty).
    File(PathBuf, Option<String>),
    /// A `#part` of this file (M11ai: its heading; not percent-decoded here).
    Here(String),
    /// Another scheme, a bare `#`, or nothing: not followed.
    Nowhere,
}

/// Where `url`, in a file in folder `base`, goes.
pub fn link_target(base: &Path, url: &str) -> LinkTarget {
    let url = url.trim();
    if ["https://", "http://", "mailto:"]
        .iter()
        .any(|s| url.starts_with(s))
    {
        return LinkTarget::Web(url.to_string());
    }
    // Another scheme (`javascript:`, `file:`, `ftp:`): not followed.
    let head = url.split(['/', '#', '?']).next().unwrap_or("");
    if head.contains(':') {
        return LinkTarget::Nowhere;
    }
    let path = url.split(['#', '?']).next().unwrap_or("");
    // The part after `#` (M11ai): a heading's anchor; an empty one is none.
    let part = url
        .split_once('#')
        .map(|(_, part)| part)
        .filter(|part| !part.is_empty());
    match (path.is_empty(), part) {
        (true, Some(part)) => LinkTarget::Here(part.to_string()),
        (true, None) => LinkTarget::Nowhere,
        (false, part) => {
            LinkTarget::File(base.join(percent_decoded(path)), part.map(str::to_string))
        }
    }
}

/// `%XX` escapes turned back into their bytes (a bad one stays as it is).
fn percent_decoded(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// How a piece of a paragraph is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Bold,
    Italic,
    Code,
    /// A link: its index in `Inline::links`.
    Link(usize),
    /// An image's alt text, in brackets.
    Image,
    /// A badge in a sentence (the reader's, M11ai): its alt text on the pill fill.
    Badge,
}

/// A paragraph's spans as one string: each styled piece's range and look, and each link's
/// range and address. A line break inside a paragraph is a space.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inline {
    pub text: String,
    pub looks: Vec<(Range<usize>, Look)>,
    pub links: Vec<(Range<usize>, String)>,
}

/// `spans` laid end to end (`Inline`), as chat draws them (the tests and scenes read it; the
/// drawing calls `inline_with`).
#[cfg(any(test, feature = "ui-shots"))]
pub fn inline(spans: &[Span]) -> Inline {
    inline_with(spans, false)
}

/// `spans` laid end to end; with `badges` (the reader's, M11ai), an image whose source is a
/// badge's is its alt text (`badge` when it has none) with `Look::Badge`, linked when it is.
pub fn inline_with(spans: &[Span], badges: bool) -> Inline {
    let mut out = Inline::default();
    for span in spans {
        let start = out.text.len();
        let look = match span {
            Span::Text(s) => {
                out.text.push_str(s);
                None
            }
            Span::Bold(s) => {
                out.text.push_str(s);
                Some(Look::Bold)
            }
            Span::Italic(s) => {
                out.text.push_str(s);
                Some(Look::Italic)
            }
            Span::Code(s) => {
                out.text.push_str(s);
                Some(Look::Code)
            }
            Span::Link { text, url } => {
                out.text.push_str(text);
                out.links.push((start..out.text.len(), url.clone()));
                Some(Look::Link(out.links.len() - 1))
            }
            Span::Image { alt, url } if badges && is_badge_src(url) => {
                out.text.push_str(badge_alt(alt));
                Some(Look::Badge)
            }
            Span::LinkedImage { alt, src, url } if badges && is_badge_src(src) => {
                out.text.push_str(badge_alt(alt));
                out.links.push((start..out.text.len(), url.clone()));
                Some(Look::Badge)
            }
            Span::Image { alt, .. } => {
                out.text.push('[');
                out.text.push_str(alt);
                out.text.push(']');
                Some(Look::Image)
            }
            Span::LinkedImage { alt, url, .. } => {
                out.text.push('[');
                out.text.push_str(alt);
                out.text.push(']');
                out.links.push((start..out.text.len(), url.clone()));
                Some(Look::Link(out.links.len() - 1))
            }
        };
        if let Some(look) = look.filter(|_| out.text.len() > start) {
            out.looks.push((start..out.text.len(), look));
        }
    }
    // A line break inside a paragraph is a space (one byte for one: the ranges hold).
    out.text = out.text.replace('\n', " ");
    out
}

/// A badge's alt text as a sentence shows it: trimmed, `badge` when empty.
fn badge_alt(alt: &str) -> &str {
    match alt.trim() {
        "" => "badge",
        alt => alt,
    }
}

/// A list item's marker: a bullet by depth (• ◦ ▪), `n.`, or a task's ☐ / ☑.
pub fn marker(item: &Md, depth: u8) -> Option<String> {
    Some(match item {
        Md::Bullet(_) => match depth {
            0 => "•",
            1 => "◦",
            _ => "▪",
        }
        .to_string(),
        Md::Numbered(n, _) => format!("{n}."),
        Md::Task(done, _) => if *done { "☑" } else { "☐" }.to_string(),
        _ => return None,
    })
}

/// Whether `md` is a list item (nested or not).
fn is_item(md: &Md) -> bool {
    matches!(
        md,
        Md::Bullet(_) | Md::Numbered(..) | Md::Task(..) | Md::Nested(..)
    )
}

/// Space above block `cur` (after `prev`), in points: none for the first, a little between
/// items of one list, `heading_above` over a heading, `heading_below` under one, `block_gap`
/// between other blocks.
pub fn gap_between(sizes: &Sizes, prev: Option<&Md>, cur: &Md) -> f32 {
    match prev {
        None => 0.0,
        Some(prev) if is_item(prev) && is_item(cur) => sizes.item_gap,
        Some(prev) => gap_after(sizes, Some(prev), cur),
    }
}

/// Space above `cur` after something that isn't its list's item (`None`: the front matter).
fn gap_after(sizes: &Sizes, prev: Option<&Md>, cur: &Md) -> f32 {
    match (prev, cur) {
        (_, Md::Heading(..)) => sizes.heading_above,
        (Some(Md::Heading(..)), _) => sizes.heading_below,
        _ => sizes.block_gap,
    }
}

const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

/// The reader's colours, from the theme (on its background, as the tab's).
#[derive(Clone, Copy, Debug)]
pub struct Colours {
    pub text: Rgb,
    pub dim: Rgb,
    pub link: Rgb,
    /// Code's fill, and code's text on it.
    pub code_bg: Rgb,
    pub code_text: Rgb,
    /// Table and heading lines, a quote's bar, a rule.
    pub line: Rgb,
    /// A table header's fill, and its text on it.
    pub header_bg: Rgb,
    pub header_text: Rgb,
    /// A code block's header strip (M11ai): its fill, and its words (the language, Copy) on it.
    pub code_header_bg: Rgb,
    pub code_header_text: Rgb,
    /// A striped table row's fill (M11ai): the text at `STRIPE` over the page; a link and dim
    /// text (an image's alt) in it, held to `TEXT_MIN` on it.
    pub stripe: Rgb,
    pub stripe_link: Rgb,
    pub stripe_dim: Rgb,
    /// A badge's pill (M11ai): its fill, its value half's (a step stronger), its text (on both)
    /// and an empty badge's dim word.
    pub pill_bg: Rgb,
    pub pill_value_bg: Rgb,
    pub pill_text: Rgb,
    pub pill_dim: Rgb,
    /// "On this page" (M11ai Task 8): the current entry's text and bar, the others' text, and a
    /// hovered entry's fill.
    pub outline_current: Rgb,
    pub outline_bar: Rgb,
    pub outline_dim: Rgb,
    pub outline_hover: Rgb,
    /// The narrow outline's floating panel (M11ai Task 9): the same, on the overlay panel's
    /// fill.
    pub outline_panel: OutlineLook,
    /// The reading-progress line (M11ai Task 9): the accent at `MARK_MIN` on the page.
    pub progress: Rgb,
    /// A callout's colours (M11ai), by `Callout::ALL`'s order.
    callouts: [CalloutColours; 5],
}

/// "On this page"'s entries on a fill (M11ai Task 9: the narrow panel's): the current entry's
/// text and bar, the others' text, and a hovered entry's fill.
#[derive(Clone, Copy, Debug)]
pub struct OutlineLook {
    pub current: Rgb,
    pub dim: Rgb,
    pub bar: Rgb,
    pub hover: Rgb,
}

/// One callout kind's colours (M11ai): its bar and icon (at `MARK_MIN` on the wash), the wash (the
/// kind's palette colour over the page at `CALLOUT_WASH`), its title (at `TEXT_MIN` on the wash)
/// and the body's text, links and dim text (the page's, held to `TEXT_MIN` on the wash).
#[derive(Clone, Copy, Debug)]
pub struct CalloutColours {
    pub bar: Rgb,
    pub wash: Rgb,
    pub title: Rgb,
    pub body: Rgb,
    pub link: Rgb,
    pub dim: Rgb,
}

impl Colours {
    pub fn of(theme: &Theme) -> Colours {
        let ui = theme.ui();
        let bg = theme.background;
        // A tint of the text over the page: it shows in a light theme too (the panel colour
        // is all but the page there).
        let code_bg = mix(bg, ui.text, 0.07);
        // A shade darker than the code's fill, in a dark theme too (spec A).
        let code_header_bg = mix(code_bg, BLACK, crate::ui_tokens::reader::CODE_HEADER_SHADE);
        let stripe = mix(bg, ui.text, crate::ui_tokens::reader::STRIPE);
        let link = ensure_contrast(ui.accent, bg, TEXT_MIN);
        let dim = ensure_contrast(ui.text_dim, bg, TEXT_MIN);
        let pill_bg = mix(bg, ui.text, crate::ui_tokens::reader::PILL_FILL);
        let pill_value_bg = mix(bg, ui.text, crate::ui_tokens::reader::PILL_VALUE_FILL);
        // Held on the stronger fill first, then on the softer one.
        let pill_text = ensure_contrast(
            ensure_contrast(ui.text, pill_value_bg, TEXT_MIN),
            pill_bg,
            TEXT_MIN,
        );
        Colours {
            text: ensure_contrast(ui.text, bg, TEXT_MIN),
            dim,
            link,
            code_bg,
            code_text: ensure_contrast(ui.text, code_bg, TEXT_MIN),
            line: ensure_contrast(ui.panel_border, bg, QUIET_LINE_MIN),
            header_bg: code_bg,
            header_text: ensure_contrast(ui.text, code_bg, TEXT_MIN),
            code_header_bg,
            code_header_text: ensure_contrast(ui.text_dim, code_header_bg, TEXT_MIN),
            stripe,
            stripe_link: ensure_contrast(link, stripe, TEXT_MIN),
            stripe_dim: ensure_contrast(dim, stripe, TEXT_MIN),
            pill_bg,
            pill_value_bg,
            pill_text,
            pill_dim: ensure_contrast(ui.text_dim, pill_bg, TEXT_MIN),
            // Held on a hovered entry's fill first, then on the page.
            outline_current: ensure_contrast(
                ensure_contrast(ui.accent, ui.panel_hover, TEXT_MIN),
                bg,
                TEXT_MIN,
            ),
            outline_bar: ensure_contrast(ui.accent, bg, MARK_MIN),
            outline_dim: ensure_contrast(
                ensure_contrast(ui.text_dim, ui.panel_hover, TEXT_MIN),
                bg,
                TEXT_MIN,
            ),
            outline_hover: ui.panel_hover,
            // As the column's, held on a hovered entry's fill first, then on the panel.
            outline_panel: OutlineLook {
                current: ensure_contrast(
                    ensure_contrast(ui.accent, ui.panel_hover, TEXT_MIN),
                    ui.panel,
                    TEXT_MIN,
                ),
                dim: ensure_contrast(
                    ensure_contrast(ui.text_dim, ui.panel_hover, TEXT_MIN),
                    ui.panel,
                    TEXT_MIN,
                ),
                bar: ensure_contrast(ui.accent, ui.panel, MARK_MIN),
                hover: ui.panel_hover,
            },
            progress: ensure_contrast(ui.accent, bg, MARK_MIN),
            callouts: Callout::ALL.map(|kind| {
                let hue = theme.palette[callout_palette(kind)];
                let wash = mix(bg, hue, crate::ui_tokens::reader::CALLOUT_WASH);
                CalloutColours {
                    bar: ensure_contrast(hue, wash, MARK_MIN),
                    wash,
                    title: ensure_contrast(hue, wash, TEXT_MIN),
                    body: ensure_contrast(ui.text, wash, TEXT_MIN),
                    link: ensure_contrast(link, wash, TEXT_MIN),
                    dim: ensure_contrast(dim, wash, TEXT_MIN),
                }
            }),
        }
    }

    /// The colours of a `kind` callout.
    pub fn callout(&self, kind: Callout) -> CalloutColours {
        let at = Callout::ALL.iter().position(|k| *k == kind).unwrap_or(0);
        self.callouts[at]
    }
}

/// The palette index of a callout kind's colour: Note blue, Tip green, Important magenta, Warning
/// yellow, Caution red.
fn callout_palette(kind: Callout) -> usize {
    match kind {
        Callout::Note => 4,
        Callout::Tip => 2,
        Callout::Important => 5,
        Callout::Warning => 3,
        Callout::Caution => 1,
    }
}

/// A callout's icon.
fn callout_glyph(kind: Callout) -> Glyph {
    match kind {
        Callout::Note => Glyph::Info,
        Callout::Tip => Glyph::Bulb,
        Callout::Important => Glyph::Report,
        Callout::Warning => Glyph::Alert,
        Callout::Caution => Glyph::Octagon,
    }
}

/// What a link click does, given its address.
pub type OnLink = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// What a code block's Copy does (M11ai), given the block's list row.
pub type OnCopy = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// How a Mermaid block is drawn (M11ac Part 2): its diagram, at its natural size in points
/// (scaled down to fit, never up), or its code with a line under it saying why not (none while
/// it renders).
#[derive(Clone)]
pub enum Shown {
    Diagram {
        image: Arc<RenderImage>,
        w: f32,
        h: f32,
    },
    Code(Option<String>),
}

/// What drawing needs besides the blocks: the colours, the code font, what a link click does,
/// a count for the links' element ids, how each Mermaid block is shown (by block index;
/// `None` or missing for every other block), the sizes, and the reader's document and looks
/// (the Preview's; `None` in agent chat).
pub struct Draw {
    pub colours: Colours,
    pub code_font: SharedString,
    pub on_link: OnLink,
    pub links: Cell<usize>,
    pub mermaid: Rc<Vec<Option<Shown>>>,
    pub sizes: Sizes,
    pub reader: Option<ReaderDraw>,
}

impl Draw {
    /// The colours to draw with now: the page's, or, inside a callout's body (the reader's),
    /// its link and dim colours held on its wash. Chat's are always the page's.
    fn ink(&self) -> Colours {
        let mut c = self.colours;
        if let Some((link, dim)) = self.reader.as_ref().and_then(|r| r.on_wash.get()) {
            c.link = link;
            c.dim = dim;
        }
        c
    }
}

/// A Markdown file as the Preview reads it (M11ai), once per version: its blocks and its plan
/// (front matter, looks, headings, outline).
pub struct ReaderDoc {
    pub blocks: Vec<Md>,
    pub plan: ReaderPlan,
}

/// What only the Preview's reader draws with: its document and the prose column's widest.
pub struct ReaderDraw {
    pub doc: Rc<ReaderDoc>,
    pub measure: f32,
    /// How much more than the side padding the reader's area keeps clear on its right (M11ai
    /// Task 8: "On this page"'s column and the gap left of it, when the column shows; else 0).
    pub aside: f32,
    /// Each coloured code block's look, by block (missing: drawn uncoloured).
    pub code: Rc<HashMap<usize, Rc<CodeLook>>>,
    /// What shows "Copied" now.
    pub copied: Option<Copied>,
    /// A code block's Copy pressed (given its row).
    pub on_copy: OnCopy,
    /// A heading's `#` pressed (given the heading's row).
    pub on_anchor_copy: OnCopy,
    /// While a callout's body is built: its link and dim colours, held on its wash (`ink`).
    pub on_wash: Cell<Option<(Rgb, Rgb)>>,
    /// The scenes' probe: each drawn heading's `#`, while it shows.
    #[cfg(feature = "ui-shots")]
    pub anchor_drawn: AnchorDrawn,
    /// The scenes' probe: each drawn row's content, as last prepainted (window points).
    #[cfg(feature = "ui-shots")]
    pub drawn: Drawn,
    /// The scenes' probe: each drawn code block's header.
    #[cfg(feature = "ui-shots")]
    pub code_drawn: CodeDrawn,
    /// The scenes' probe: each drawn callout's title.
    #[cfg(feature = "ui-shots")]
    pub callout_drawn: CalloutDrawn,
    /// The scenes' probe: the front matter as drawn, and each drawn badge row's pills.
    #[cfg(feature = "ui-shots")]
    pub front_drawn: FrontDrawn,
    #[cfg(feature = "ui-shots")]
    pub pills_drawn: PillsDrawn,
}

/// A code block coloured (M11ai, `md_reader::code_runs`): its text as shown (tabs widened) and
/// its colour runs on that text.
pub struct CodeLook {
    pub shown: SharedString,
    pub runs: Vec<(Range<usize>, Rgb)>,
}

/// What was copied last, shown as "Copied" for `reader::COPIED` (M11ai): a code block's Copy, or
/// a heading's link (Task 7), by list row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Copied {
    Code(usize),
    Anchor(usize),
}

/// A code block's header as last drawn (the scenes' probe): the language it names, the Copy
/// button's word, and the button's bounds (window points).
#[cfg(feature = "ui-shots")]
#[derive(Clone, Debug)]
pub struct CodeProbe {
    pub lang: Option<String>,
    pub label: &'static str,
    pub copy: gpui::Bounds<gpui::Pixels>,
}

/// Each drawn code block's header, by list row.
#[cfg(feature = "ui-shots")]
pub type CodeDrawn = Rc<std::cell::RefCell<std::collections::BTreeMap<usize, CodeProbe>>>;

/// Each drawn callout's title word, by list row (the scenes read it).
#[cfg(feature = "ui-shots")]
pub type CalloutDrawn = Rc<std::cell::RefCell<std::collections::BTreeMap<usize, &'static str>>>;

/// The front matter as last drawn (the scenes' probe): its key/value rows and its `+ N more`
/// row as shown, or a raw block's header word; and its bounds (window points).
#[cfg(feature = "ui-shots")]
#[derive(Clone, Debug)]
pub struct FrontProbe {
    pub rows: Vec<(String, String)>,
    pub more: Option<String>,
    pub raw: Option<&'static str>,
    pub bounds: gpui::Bounds<gpui::Pixels>,
}

#[cfg(feature = "ui-shots")]
pub type FrontDrawn = Rc<std::cell::RefCell<Option<FrontProbe>>>;

/// A badge row's pill as last drawn (the scenes' probe): its words, whether it is the dim
/// `badge`, and its bounds (window points).
#[cfg(feature = "ui-shots")]
#[derive(Clone, Debug)]
pub struct PillProbe {
    pub label: String,
    pub value: Option<String>,
    pub dim: bool,
    pub bounds: gpui::Bounds<gpui::Pixels>,
}

/// Each drawn badge row's pills, by (list row, pill).
#[cfg(feature = "ui-shots")]
pub type PillsDrawn = Rc<std::cell::RefCell<std::collections::BTreeMap<(usize, usize), PillProbe>>>;

/// A heading's `#` as last drawn (the scenes' probe; only while it shows): its word (`#`, or
/// `Copied`) and its bounds (window points).
#[cfg(feature = "ui-shots")]
#[derive(Clone, Debug)]
pub struct AnchorProbe {
    pub label: &'static str,
    pub bounds: gpui::Bounds<gpui::Pixels>,
}

/// Each drawn heading's `#`, by list row.
#[cfg(feature = "ui-shots")]
pub type AnchorDrawn = Rc<std::cell::RefCell<std::collections::BTreeMap<usize, AnchorProbe>>>;

/// Each drawn row's content bounds, by list row (the scenes read it).
#[cfg(feature = "ui-shots")]
pub type Drawn =
    Rc<std::cell::RefCell<std::collections::BTreeMap<usize, gpui::Bounds<gpui::Pixels>>>>;

/// Row `row` of the Preview: the front matter or a block, in the centred column. The row is
/// the Preview's full width, `TERMINAL_PADDING.0` in at each side (its `.1` over the first row
/// and under the last), with the space above it (`gap_between`); inside that, the reader's area,
/// its content centred: prose at most `measure` wide; code blocks and tables (`wide`) at least
/// the column and up to the area; a diagram at most the column. Its links' element ids start
/// from the row's index, so they stay the same from frame to frame (a click's press and release
/// meet).
pub fn reader_row(row: usize, draw: &Draw) -> AnyElement {
    let Some(reader) = &draw.reader else {
        return div().into_any_element();
    };
    let (doc, rows) = (&reader.doc, reader.doc.plan.rows());
    if row >= rows {
        return div().into_any_element();
    }
    draw.links.set(row << 20);
    let s = &draw.sizes;
    let prose = |e: AnyElement| {
        div()
            .w_full()
            .max_w(px(reader.measure))
            .child(e)
            .into_any_element()
    };
    let content = match doc.plan.row_kind(row) {
        RowKind::Front => match &doc.plan.front {
            Some(Front::Table(t)) => prose(front_table(t, draw, reader)),
            Some(Front::Raw(text)) => wide(reader.measure, front_raw(text, draw, reader)),
            None => div().into_any_element(),
        },
        RowKind::Block(b) => {
            let Some(md) = doc.blocks.get(b) else {
                return div().into_any_element();
            };
            match (draw.mermaid.get(b), md) {
                (Some(Some(shown @ Shown::Diagram { .. })), _) => {
                    prose(mermaid_block(shown, draw, || div().into_any_element()))
                }
                (Some(Some(shown)), _) => wide(
                    reader.measure,
                    mermaid_block(shown, draw, || reader_code(row, b, md, draw, reader)),
                ),
                (_, Md::Table { align, head, rows }) => {
                    let min = head.len().max(1) as f32 * TABLE_COL_MIN;
                    let mut scroll = div()
                        .id(("md-table", row))
                        .w_full()
                        .overflow_x_scroll()
                        .child(table(align, head, rows, Some(min), draw));
                    // A wheel's up and down scroll the page, not the table.
                    scroll.style().restrict_scroll_to_axis = Some(true);
                    wide(reader.measure, scroll.into_any_element())
                }
                (_, Md::Code { .. }) => wide(reader.measure, reader_code(row, b, md, draw, reader)),
                (_, Md::Heading(level, _)) => prose(reader_heading(row, *level, md, draw, reader)),
                _ => match doc.plan.looks.get(b) {
                    Some(BlockLook::Callout(kind, body)) => {
                        prose(reader_callout(row, *kind, body, draw, reader))
                    }
                    Some(BlockLook::Badges(badges)) => {
                        prose(reader_badges(row, badges, draw, reader))
                    }
                    _ => prose(block(md, draw)),
                },
            }
        }
    };
    let (side, end) = TERMINAL_PADDING;
    let top = row_top(doc, s, row);
    let bottom = if row + 1 == rows { end } else { 0.0 };
    // The reader's area (less "On this page"'s column and gap when it shows), its content
    // centred along a row: the content's width is settled before its height is
    // measured (centred in a column, taffy measured a paragraph's height at the area's width).
    // A heading's row is its `#`'s hover group (M11ai: the `#` shows while it is hovered).
    let heading = match doc.plan.row_kind(row) {
        RowKind::Block(b) => matches!(doc.blocks.get(b), Some(Md::Heading(..))),
        RowKind::Front => false,
    };
    let area = div()
        .w_full()
        .flex()
        .flex_row()
        .justify_center()
        .when(heading, |d| d.group(ANCHOR_GROUP));
    #[cfg(feature = "ui-shots")]
    let area = {
        let drawn = reader.drawn.clone();
        area.on_children_prepainted(move |bounds, _, _| {
            if let Some(b) = bounds.first() {
                drawn.borrow_mut().insert(row, *b);
            }
        })
    };
    div()
        .w_full()
        .pl(px(side))
        .pr(px(side + reader.aside))
        .pt(px(top))
        .pb(px(bottom))
        .text_size(px(s.body))
        .line_height(relative(s.line))
        .text_color(hsla(draw.ink().text))
        .child(area.child(content))
        .into_any_element()
}

/// The space over row `row`'s content in the reader, in points: `TERMINAL_PADDING.1` over the
/// first row, else the gap above its block (`gap_between`; after the front matter, as after no
/// block of its list). A jump to a heading's row goes `md_reader::jump_offset` of this into it.
pub fn row_top(doc: &ReaderDoc, s: &Sizes, row: usize) -> f32 {
    if row == 0 {
        return TERMINAL_PADDING.1;
    }
    match doc.plan.row_kind(row) {
        RowKind::Front => 0.0,
        RowKind::Block(b) => match (doc.blocks.get(b), b.checked_sub(1)) {
            (Some(md), Some(prev)) => gap_between(s, doc.blocks.get(prev), md),
            (Some(md), None) => gap_after(s, None, md),
            (None, _) => 0.0,
        },
    }
}

/// The hover group of a heading's row (its `#` shows while it is hovered).
const ANCHOR_GROUP: &str = "md-anchor";

/// A heading in the reader (M11ai): the heading, and in the margin `ANCHOR_GAP` left of its
/// first line a dim `#`, shown while its row (or the `#` itself) is under the pointer; a click
/// on it copies the link to the heading (`on_anchor_copy`), and it reads `Copied` with a tick
/// while `copied` says so (shown then without a hover). Its element id is from `row`; it sits
/// outside the heading's text, so its click is not the text's.
fn reader_heading(row: usize, level: u8, md: &Md, draw: &Draw, reader: &ReaderDraw) -> AnyElement {
    use crate::ui_tokens::reader::{ANCHOR_GAP, ANCHOR_TEXT, COPY_GAP};
    let (c, s) = (draw.ink(), &draw.sizes);
    let size = heading_size(s, level);
    let copied = reader.copied == Some(Copied::Anchor(row));
    let (label, inner) = if copied {
        let word = "Copied";
        let inner = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(COPY_GAP))
            .text_size(px(ANCHOR_TEXT))
            .child(glyph(Glyph::Check, c.dim))
            .child(word);
        (word, inner)
    } else {
        ("#", div().child("#"))
    };
    #[cfg(feature = "ui-shots")]
    let inner = {
        let probe = reader.anchor_drawn.clone();
        // Recorded as painted: a hidden `#` paints nothing.
        inner.relative().child(
            gpui::canvas(
                |bounds, _, _| bounds,
                move |_, bounds, _, _| {
                    probe
                        .borrow_mut()
                        .insert(row, AnchorProbe { label, bounds });
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    };
    #[cfg(not(feature = "ui-shots"))]
    let _ = label;
    let on_copy = reader.on_anchor_copy.clone();
    let mark = div()
        .id(("md-anchor", row))
        .absolute()
        .top_0()
        .right_full()
        // The gap is the `#`'s own, so the pointer goes from the heading onto it unbroken.
        .pr(px(ANCHOR_GAP))
        .h(px(size * s.heading_line))
        .flex()
        .flex_row()
        .items_center()
        .font_weight(FontWeight::NORMAL)
        .text_color(hsla(c.dim))
        .cursor_pointer()
        .when(!copied, |d| {
            d.invisible()
                .group_hover(ANCHOR_GROUP, |s| s.visible())
                .hover(|s| s.visible())
        })
        .on_click(move |_, window, cx| on_copy(row, window, cx))
        .child(inner);
    div()
        .relative()
        .w_full()
        .text_size(px(size))
        .line_height(relative(s.heading_line))
        .child(block(md, draw))
        .child(mark)
        .into_any_element()
}

/// A heading's text size at `level` (1 to 6).
fn heading_size(s: &Sizes, level: u8) -> f32 {
    s.headings[(level as usize).clamp(1, 6) - 1]
}

/// A code block or table in the reader (plan-time call 7): as wide as its content, but at
/// least the column (`measure`, or the area when that is narrower) and at most the area. (The
/// spacer, as wide as the column and no height, makes its content at least that wide; the area
/// caps it.)
fn wide(measure: f32, e: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .min_w(px(0.0))
        .max_w_full()
        .child(div().w(px(measure)).max_w_full().h(px(0.0)))
        .child(e)
        .into_any_element()
}

/// A callout in the reader (M11ai): a `CALLOUT_BAR` bar on the left in the kind's colour, the
/// kind's wash behind, `CALLOUT_PAD` inside, `CALLOUT_RADIUS` corners; a title line (the kind's
/// icon and word, semibold, in its title colour), then the body's blocks in the text colour with
/// the reader's gaps, its links and dim text in the callout's (held on its wash). A body-less
/// callout is its title alone.
fn reader_callout(
    #[cfg_attr(not(feature = "ui-shots"), allow(unused_variables))] row: usize,
    kind: Callout,
    body: &[Md],
    draw: &Draw,
    reader: &ReaderDraw,
) -> AnyElement {
    use crate::ui_tokens::reader::{CALLOUT_BAR, CALLOUT_GAP, CALLOUT_PAD, CALLOUT_RADIUS};
    let (c, s) = (draw.colours.callout(kind), &draw.sizes);
    let title = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(CALLOUT_GAP))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(hsla(c.title))
        .child(glyph(callout_glyph(kind), c.bar))
        .child(kind.title());
    #[cfg(feature = "ui-shots")]
    let title = {
        let probe = reader.callout_drawn.clone();
        title.child(
            gpui::canvas(
                move |_, _, _| {
                    probe.borrow_mut().insert(row, kind.title());
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_0(),
        )
    };
    div()
        .w_full()
        .flex()
        .flex_col()
        .p(px(CALLOUT_PAD))
        .rounded(px(CALLOUT_RADIUS))
        .bg(hsla(c.wash))
        .border_l(px(CALLOUT_BAR))
        .border_color(hsla(c.bar))
        .text_color(hsla(c.body))
        .child(title)
        .when(!body.is_empty(), |d| {
            // The body's links and dim text in the callout's colours, held on its wash; the
            // page's again once its elements are built.
            let was = reader.on_wash.replace(Some((c.link, c.dim)));
            let blocks = blocks_of(body, draw);
            reader.on_wash.set(was);
            d.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .mt(px(s.item_gap))
                    .children(blocks),
            )
        })
        .into_any_element()
}

/// A code block in the reader (M11ai): a header strip on its top edge (a shade off the code's
/// fill: the fence's language on the left, as the fence names it, nothing when it names none;
/// Copy on the right, dim, brighter under the pointer; "Copied" with a tick while `copied` says
/// so), then its code: coloured once the colour job's answer for block `b` is in, else in the
/// code colour. The button's element id is from `row`.
fn reader_code(row: usize, b: usize, md: &Md, draw: &Draw, reader: &ReaderDraw) -> AnyElement {
    use crate::ui_tokens::reader::COPY_GAP;
    let Md::Code { lang, text: code } = md else {
        return block(md, draw);
    };
    let c = draw.ink();
    let lang = fence_lang(lang.as_deref()).map(str::to_string);
    let copied = reader.copied == Some(Copied::Code(row));
    let (word, kind) = if copied {
        ("Copied", Glyph::Check)
    } else {
        ("Copy", Glyph::Copy)
    };
    let (dim, bright) = (
        c.code_header_text,
        ensure_contrast(c.text, c.code_header_bg, TEXT_MIN),
    );
    let on_copy = reader.on_copy.clone();
    // One `group_hover` per element (GPUI keeps the last): the dim icon hides, the bright one
    // shows.
    let icon = div()
        .relative()
        .flex_none()
        .size(px(ICON))
        .child(
            glyph(kind, dim)
                .absolute()
                .group_hover(COPY_GROUP, |s| s.invisible()),
        )
        .child(
            glyph(kind, bright)
                .absolute()
                .invisible()
                .group_hover(COPY_GROUP, |s| s.visible()),
        );
    let button = div()
        .id(("md-copy", row))
        .group(COPY_GROUP)
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(COPY_GAP))
        .cursor_pointer()
        .text_color(hsla(dim))
        .hover(move |s| s.text_color(hsla(bright)))
        .on_click(move |_, window, cx| on_copy(row, window, cx))
        .child(icon)
        .child(word);
    #[cfg(feature = "ui-shots")]
    let button = {
        let (probe, lang) = (reader.code_drawn.clone(), lang.clone());
        button.child(
            gpui::canvas(
                move |bounds, _, _| {
                    probe.borrow_mut().insert(
                        row,
                        CodeProbe {
                            lang,
                            label: word,
                            copy: bounds,
                        },
                    );
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    };
    let text: AnyElement = match reader.code.get(&b) {
        Some(look) => {
            let runs = look.runs.iter().map(|(range, rgb)| {
                let style = HighlightStyle {
                    color: Some(hsla(*rgb)),
                    ..Default::default()
                };
                (range.clone(), style)
            });
            StyledText::new(look.shown.clone())
                .with_highlights(runs.collect::<Vec<_>>())
                .into_any_element()
        }
        None => {
            let shown: Vec<String> = code.lines().map(expand_tabs).collect();
            SharedString::from(shown.join("\n")).into_any_element()
        }
    };
    code_frame(lang, Some(button.into_any_element()), text, draw)
}

/// A reader's code block round `text` (M11ai): the header strip on its top edge (a shade off
/// the code's fill; `label` on the left, dim, nothing when `None`; `button` on the right, if
/// any), then the text in the code's font, padding and colour.
fn code_frame(
    label: Option<String>,
    button: Option<AnyElement>,
    text: AnyElement,
    draw: &Draw,
) -> AnyElement {
    use crate::ui_tokens::reader::{CODE_BORDER, CODE_HEADER, CODE_HEADER_TEXT};
    let (c, s) = (draw.ink(), &draw.sizes);
    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .h(px(CODE_HEADER))
        .px(px(s.code_pad.0))
        // Inside the block's border: its corners that much less round.
        .rounded_t(px((s.code_radius - CODE_BORDER).max(0.0)))
        .bg(hsla(c.code_header_bg))
        .text_size(px(CODE_HEADER_TEXT))
        .text_color(hsla(c.code_header_text))
        .child(div().children(label))
        .children(button);
    div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(s.code_radius))
        .bg(hsla(c.code_bg))
        .border(px(CODE_BORDER))
        .border_color(hsla(c.line))
        .child(header)
        .child(
            div()
                .w_full()
                .px(px(s.code_pad.0))
                .py(px(s.code_pad.1))
                .font_family(draw.code_font.clone())
                .text_size(px(s.code))
                .line_height(relative(s.code_line))
                .text_color(hsla(c.code_text))
                .child(text),
        )
        .into_any_element()
}

/// The Copy buttons' hover group (one per button: the nearest is the one hovered).
const COPY_GROUP: &str = "md-copy";

/// The words over a front matter that does not read as `key: value` lines (M11ai).
const FRONT_RAW: &str = "front matter";

/// The front matter as a small table (M11ai spec C): a row per key, the key dim and the value
/// (wrapping) in `FRONT_TEXT`, the table's lines and corners, no stripes; the key in
/// `FRONT_GRID.0` of `FRONT_GRID.1` equal columns, its value in the rest; then `+ N more`
/// (dim, the full width) for the keys past `FRONT_ROWS`.
fn front_table(
    t: &crate::md_reader::FrontTable,
    draw: &Draw,
    #[cfg_attr(not(feature = "ui-shots"), allow(unused_variables))] reader: &ReaderDraw,
) -> AnyElement {
    use crate::ui_tokens::reader::{FRONT_GRID, FRONT_TEXT, TABLE_CELL, TABLE_RADIUS};
    let c = draw.ink();
    let (key_cols, cols) = FRONT_GRID;
    let more = (t.more > 0).then(|| more_label(t.more));
    let cell = |top: bool| {
        div()
            .min_w(px(0.0))
            .px(px(TABLE_CELL.0))
            .py(px(TABLE_CELL.1))
            .when(!top, |d| d.border_t_1())
            .border_color(hsla(c.line))
    };
    let mut cells: Vec<AnyElement> = Vec::new();
    for (i, (key, value)) in t.rows.iter().enumerate() {
        cells.push(
            cell(i == 0)
                .col_span(key_cols)
                .border_r_1()
                .text_color(hsla(c.dim))
                .child(key.clone())
                .into_any_element(),
        );
        cells.push(
            cell(i == 0)
                .col_span(cols - key_cols)
                .child(value.clone())
                .into_any_element(),
        );
    }
    if let Some(more) = &more {
        cells.push(
            cell(t.rows.is_empty())
                .col_span_full()
                .text_color(hsla(c.dim))
                .child(more.clone())
                .into_any_element(),
        );
    }
    let grid = div()
        .w_full()
        .grid()
        .grid_cols(cols)
        .border_1()
        .border_color(hsla(c.line))
        .rounded(px(TABLE_RADIUS))
        .overflow_hidden()
        .text_size(px(FRONT_TEXT))
        .children(cells);
    #[cfg(feature = "ui-shots")]
    let grid = {
        let (probe, rows) = (reader.front_drawn.clone(), t.rows.clone());
        div().relative().w_full().child(grid).child(
            gpui::canvas(
                move |bounds, _, _| {
                    *probe.borrow_mut() = Some(FrontProbe {
                        rows,
                        more,
                        raw: None,
                        bounds,
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    };
    grid.into_any_element()
}

/// Front matter that does not read as `key: value` lines (M11ai): a code block headed
/// `front matter`, no Copy.
fn front_raw(
    text: &str,
    draw: &Draw,
    #[cfg_attr(not(feature = "ui-shots"), allow(unused_variables))] reader: &ReaderDraw,
) -> AnyElement {
    let shown: Vec<String> = text.lines().map(expand_tabs).collect();
    let block = code_frame(
        Some(FRONT_RAW.to_string()),
        None,
        SharedString::from(shown.join("\n")).into_any_element(),
        draw,
    );
    #[cfg(feature = "ui-shots")]
    let block = {
        let probe = reader.front_drawn.clone();
        div()
            .relative()
            .w_full()
            .child(block)
            .child(
                gpui::canvas(
                    move |bounds, _, _| {
                        *probe.borrow_mut() = Some(FrontProbe {
                            rows: Vec::new(),
                            more: None,
                            raw: Some(FRONT_RAW),
                            bounds,
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    };
    block
}

/// A badge row (M11ai spec C): its pills in a wrapping row, `PILL_GAP` apart.
fn reader_badges(row: usize, badges: &[Badge], draw: &Draw, reader: &ReaderDraw) -> AnyElement {
    use crate::ui_tokens::reader::PILL_GAP;
    div()
        .w_full()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(PILL_GAP))
        .children(
            badges
                .iter()
                .enumerate()
                .map(|(i, badge)| pill_of(row, i, badge, draw, reader)),
        )
        .into_any_element()
}

/// A badge's pill (M11ai): `PILL_H` high, fully rounded, `PILL_TEXT` on one line, `PILL_PAD` at
/// its sides; its label on `pill_bg` (dim when the alt text was empty), its value, if any, on
/// `pill_value_bg`. A linked pill follows its link through `on_link` (its image never opened).
fn pill_of(
    #[cfg_attr(not(feature = "ui-shots"), allow(unused_variables))] row: usize,
    #[cfg_attr(not(feature = "ui-shots"), allow(unused_variables))] i: usize,
    badge: &Badge,
    draw: &Draw,
    #[cfg_attr(not(feature = "ui-shots"), allow(unused_variables))] reader: &ReaderDraw,
) -> AnyElement {
    use crate::ui_tokens::reader::{PILL_H, PILL_PAD, PILL_TEXT};
    let c = draw.ink();
    let round = px(PILL_H / 2.0);
    let two = badge.value.is_some();
    let part = |words: String, fill: Rgb, colour: Rgb| {
        div()
            .h_full()
            .flex()
            .items_center()
            .px(px(PILL_PAD))
            .bg(hsla(fill))
            .text_color(hsla(colour))
            .child(words)
    };
    let label = part(
        badge.label.clone(),
        c.pill_bg,
        if badge.empty { c.pill_dim } else { c.pill_text },
    );
    let label = if two {
        label.rounded_l(round)
    } else {
        label.rounded(round)
    };
    let value = badge
        .value
        .clone()
        .map(|v| part(v, c.pill_value_bg, c.pill_text).rounded_r(round));
    let n = draw.links.get();
    draw.links.set(n + 1);
    let pill = div()
        .id(("md-pill", n))
        .relative()
        .flex_none()
        .flex()
        .flex_row()
        .h(px(PILL_H))
        .text_size(px(PILL_TEXT))
        .line_height(px(PILL_H))
        .whitespace_nowrap()
        .child(label)
        .children(value);
    let pill = match badge.url.clone() {
        Some(url) => {
            let on_link = draw.on_link.clone();
            pill.cursor_pointer()
                .on_click(move |_, window, cx| on_link(url.clone(), window, cx))
        }
        None => pill,
    };
    #[cfg(feature = "ui-shots")]
    let pill = {
        let probe = reader.pills_drawn.clone();
        let (label, value, dim) = (badge.label.clone(), badge.value.clone(), badge.empty);
        pill.child(
            gpui::canvas(
                move |bounds, _, _| {
                    probe.borrow_mut().insert(
                        (row, i),
                        PillProbe {
                            label,
                            value,
                            dim,
                            bounds,
                        },
                    );
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    };
    pill.into_any_element()
}

/// `blocks` as one column with no page padding (M11aa: an agent's message in a chat row): the
/// reader's blocks (`blocks_of`) one under another. Its links' element ids start from
/// `base << 20` (a chat passes its item's index), so they stay the same from frame to frame.
/// Mermaid blocks show as code (R7).
pub fn column(blocks: &[Md], base: usize, draw: &Draw) -> AnyElement {
    draw.links.set(base << 20);
    div()
        .w_full()
        .flex()
        .flex_col()
        .children(blocks_of(blocks, draw))
        .into_any_element()
}

/// A Mermaid block (M11ac Part 2): its diagram centred, at its natural width or the column's,
/// whichever is less, its height kept in proportion; or its code (`code`: the reader's, with
/// its header), with the reason under it.
fn mermaid_block(shown: &Shown, draw: &Draw, code: impl FnOnce() -> AnyElement) -> AnyElement {
    use crate::ui_tokens::reader::{DIAGRAM_RADIUS, NOTE_GAP, NOTE_LINE, NOTE_TEXT};
    match shown {
        Shown::Diagram { image, w, h } => div()
            .w_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .max_w(px(*w))
                    .aspect_ratio(w / h.max(1.0))
                    .child(img(image.clone()).size_full().rounded(px(DIAGRAM_RADIUS))),
            )
            .into_any_element(),
        Shown::Code(note) => div()
            .w_full()
            .flex()
            .flex_col()
            .child(code())
            .when_some(note.clone(), |d, note| {
                d.child(
                    div()
                        .mt(px(NOTE_GAP))
                        .text_size(px(NOTE_TEXT))
                        .line_height(relative(NOTE_LINE))
                        .text_color(hsla(draw.ink().dim))
                        .child(note),
                )
            })
            .into_any_element(),
    }
}

fn blocks_of(blocks: &[Md], draw: &Draw) -> Vec<AnyElement> {
    let mut prev = None;
    blocks
        .iter()
        .map(|md| {
            let gap = gap_between(&draw.sizes, prev, md);
            prev = Some(md);
            div()
                .w_full()
                .mt(px(gap))
                .child(block(md, draw))
                .into_any_element()
        })
        .collect()
}

fn block(md: &Md, draw: &Draw) -> AnyElement {
    let (c, s) = (draw.ink(), &draw.sizes);
    match md {
        Md::Para(spans) => text(spans, draw),
        Md::Heading(level, spans) => {
            let size = heading_size(s, *level);
            div()
                .w_full()
                .text_size(px(size))
                .line_height(relative(s.heading_line))
                .font_weight(FontWeight::SEMIBOLD)
                // The reader's H6 is dim (chat's is not).
                .when(*level == 6 && draw.reader.is_some(), |d| {
                    d.text_color(hsla(c.dim))
                })
                .when(*level <= 2, |d| {
                    d.pb(px(s.rule_below))
                        .border_b_1()
                        .border_color(hsla(c.line))
                })
                .child(text(spans, draw))
                .into_any_element()
        }
        Md::Bullet(_) | Md::Numbered(..) | Md::Task(..) => item(md, 0, draw),
        Md::Nested(depth, inner) => item(inner, *depth, draw),
        Md::Code { text: code, .. } => {
            let shown: Vec<String> = code.lines().map(expand_tabs).collect();
            div()
                .w_full()
                .px(px(s.code_pad.0))
                .py(px(s.code_pad.1))
                .rounded(px(s.code_radius))
                .bg(hsla(c.code_bg))
                .border_1()
                .border_color(hsla(c.line))
                .font_family(draw.code_font.clone())
                .text_size(px(s.code))
                .line_height(relative(s.code_line))
                .text_color(hsla(c.code_text))
                .child(SharedString::from(shown.join("\n")))
                .into_any_element()
        }
        Md::Quote(inner) => div()
            .w_full()
            .flex()
            .flex_col()
            .pl(px(s.quote_pad))
            .border_l(px(s.quote_bar))
            .border_color(hsla(c.line))
            .text_color(hsla(c.dim))
            .children(blocks_of(inner, draw))
            .into_any_element(),
        Md::Table { align, head, rows } => table(align, head, rows, None, draw),
        Md::Rule => div()
            .w_full()
            .my(px(s.rule_margin))
            .h(px(s.rule_line))
            .bg(hsla(c.line))
            .into_any_element(),
    }
}

/// A list item `depth` levels in: its marker in a column of its own, its text beside it.
fn item(md: &Md, depth: u8, draw: &Draw) -> AnyElement {
    let spans = match md {
        Md::Bullet(s) | Md::Numbered(_, s) | Md::Task(_, s) => s,
        other => return block(other, draw),
    };
    let numbered = matches!(md, Md::Numbered(..));
    let s = &draw.sizes;
    div()
        .w_full()
        .flex()
        .flex_row()
        .pl(px(depth as f32 * s.indent))
        .child(
            div()
                .flex_none()
                .min_w(px(s.indent))
                .pr(px(if numbered { s.marker_gap } else { 0.0 }))
                .text_color(hsla(draw.ink().dim))
                .children(marker(md, depth)),
        )
        .child(div().flex_1().min_w(px(0.0)).child(text(spans, draw)))
        .into_any_element()
}

/// Whether body row `r` (from 0) of a reader's table is striped: the 2nd, 4th, …
fn striped(r: usize) -> bool {
    r % 2 == 1
}

/// The front matter's last row, for the `n` keys it does not show.
fn more_label(n: usize) -> String {
    format!("+ {n} more")
}

/// A table: a grid of equal columns, the header filled and bold, lines between the cells; at
/// least `min` wide (the reader's: its columns at least `TABLE_COL_MIN` each).
fn table(
    align: &[Align],
    head: &[Vec<Span>],
    rows: &[Vec<Vec<Span>>],
    min: Option<f32>,
    draw: &Draw,
) -> AnyElement {
    use crate::ui_tokens::reader::TABLE_RADIUS;
    let c = draw.ink();
    let n = head.len().max(1);
    // The reader's tables (M11ai): more room in a cell, every 2nd body row striped; chat's as
    // M11ac drew them.
    let reader = draw.reader.is_some();
    let (pad_x, pad_y) = draw.sizes.table_cell;
    let cell = |spans: &[Span], col: usize, row: usize, header: bool| {
        let d = div()
            .min_w(px(0.0))
            .px(px(pad_x))
            .py(px(pad_y))
            .when(col + 1 < n, |d| d.border_r_1())
            .when(row > 0, |d| d.border_t_1())
            .border_color(hsla(c.line))
            .when(header, |d| {
                d.bg(hsla(c.header_bg))
                    .text_color(hsla(c.header_text))
                    .font_weight(FontWeight::SEMIBOLD)
            });
        let on_stripe = reader && !header && striped(row - 1);
        let d = d.when(on_stripe, |d| d.bg(hsla(c.stripe)));
        let d = match align.get(col) {
            Some(Align::Center) => d.text_center(),
            Some(Align::Right) => d.text_right(),
            _ => d,
        };
        d.child(text_on(spans, draw, on_stripe)).into_any_element()
    };
    let mut cells: Vec<AnyElement> = head
        .iter()
        .enumerate()
        .map(|(col, s)| cell(s, col, 0, true))
        .collect();
    for (r, row) in rows.iter().enumerate() {
        cells.extend(
            row.iter()
                .enumerate()
                .map(|(col, s)| cell(s, col, r + 1, false)),
        );
    }
    div()
        .w_full()
        .grid()
        .grid_cols(n.min(u16::MAX as usize) as u16)
        .when_some(min, |d, min| d.min_w(px(min)))
        .border_1()
        .border_color(hsla(c.line))
        .rounded(px(TABLE_RADIUS))
        .overflow_hidden()
        .children(cells)
        .into_any_element()
}

/// A paragraph's spans as one wrapping text: bold, italic, code in the terminal's font on its
/// fill, links in the link colour, underlined and clickable, images' alt text dim.
fn text(spans: &[Span], draw: &Draw) -> AnyElement {
    text_on(spans, draw, false)
}

/// `text`, on a striped table row (`on_stripe`, the reader's): links and images' alt text in
/// their colours held to the text floor on the stripe.
fn text_on(spans: &[Span], draw: &Draw, on_stripe: bool) -> AnyElement {
    let c = draw.ink();
    let (link, dim) = if on_stripe {
        (c.stripe_link, c.stripe_dim)
    } else {
        (c.link, c.dim)
    };
    // A badge in a sentence has its look in the reader only (chat's stays an image's).
    let laid = inline_with(spans, draw.reader.is_some());
    let highlights = laid.looks.iter().map(|(range, look)| {
        let style = match look {
            Look::Bold => HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..Default::default()
            },
            Look::Italic => HighlightStyle {
                font_style: Some(FontStyle::Italic),
                ..Default::default()
            },
            Look::Code => HighlightStyle {
                color: Some(hsla(c.code_text)),
                background_color: Some(hsla(c.code_bg)),
                ..Default::default()
            },
            Look::Link(_) => HighlightStyle {
                color: Some(hsla(link)),
                underline: Some(UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(hsla(link)),
                    wavy: false,
                }),
                ..Default::default()
            },
            Look::Image => HighlightStyle {
                color: Some(hsla(dim)),
                font_style: Some(FontStyle::Italic),
                ..Default::default()
            },
            Look::Badge => HighlightStyle {
                color: Some(hsla(c.pill_text)),
                background_color: Some(hsla(c.pill_bg)),
                ..Default::default()
            },
        };
        (range.clone(), style)
    });
    let code: Vec<(Range<usize>, SharedString)> = laid
        .looks
        .iter()
        .filter(|(_, look)| *look == Look::Code)
        .map(|(range, _)| (range.clone(), draw.code_font.clone()))
        .collect();
    let styled = StyledText::new(laid.text)
        .with_highlights(highlights.collect::<Vec<_>>())
        .with_font_family_overrides(code);
    if laid.links.is_empty() {
        return div().w_full().child(styled).into_any_element();
    }
    let n = draw.links.get();
    draw.links.set(n + 1);
    let (ranges, urls): (Vec<Range<usize>>, Vec<String>) = laid.links.into_iter().unzip();
    let on_link = draw.on_link.clone();
    div()
        .w_full()
        .child(InteractiveText::new(("md-links", n), styled).on_click(
            ranges,
            move |i, window, cx| {
                if let Some(url) = urls.get(i) {
                    on_link(url.clone(), window, cx);
                }
            },
        ))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linked_image_reads_as_its_alt_linked() {
        let i = inline(&[Span::LinkedImage {
            alt: "ci".into(),
            src: "s".into(),
            url: "u".into(),
        }]);
        assert_eq!(i.text, "[ci]");
        assert_eq!(i.looks, vec![(0..4, Look::Link(0))]);
        assert_eq!(i.links, vec![(0..4, "u".to_string())]);
    }

    fn t(s: &str) -> Span {
        Span::Text(s.into())
    }

    /// Every built-in theme: the reader's text, dim text, links and code meet the text floor
    /// on what they're drawn on.
    #[test]
    fn the_reader_s_colours_meet_the_text_floor_in_every_theme() {
        use crate::ui_tokens::contrast;
        for (name, theme) in crate::theme::every_theme() {
            let c = Colours::of(&theme);
            let bg = theme.background;
            for (what, fg, on) in [
                ("text", c.text, bg),
                ("dim", c.dim, bg),
                ("link", c.link, bg),
                ("code", c.code_text, c.code_bg),
                ("table header", c.header_text, c.header_bg),
                ("code header", c.code_header_text, c.code_header_bg),
                ("table text on a stripe", c.text, c.stripe),
                ("link on a stripe", c.stripe_link, c.stripe),
                ("dim on a stripe", c.stripe_dim, c.stripe),
                ("pill", c.pill_text, c.pill_bg),
                ("pill value", c.pill_text, c.pill_value_bg),
                ("empty pill", c.pill_dim, c.pill_bg),
            ] {
                let got = contrast(fg, on);
                assert!(got >= TEXT_MIN - 0.01, "{name}: {what} {got}");
            }
            // The code header is a shade off the code's fill.
            assert_ne!(c.code_header_bg, c.code_bg, "{name}");
        }
    }

    /// Every theme (M11ai Task 8): "On this page"'s current entry (the accent) and the other
    /// entries (dim) meet the text floor on the page and on a hovered entry's fill (the panel
    /// hover); the current entry's bar the mark floor on the page.
    #[test]
    fn the_outline_s_colours_meet_their_floors_in_every_theme() {
        use crate::ui_tokens::contrast;
        for (name, theme) in crate::theme::every_theme() {
            let c = Colours::of(&theme);
            let bg = theme.background;
            assert_eq!(
                c.outline_hover,
                theme.ui().panel_hover,
                "{name}: the hover fill"
            );
            for (what, fg, on, min) in [
                ("current entry", c.outline_current, bg, TEXT_MIN),
                (
                    "current entry hovered",
                    c.outline_current,
                    c.outline_hover,
                    TEXT_MIN,
                ),
                ("entry", c.outline_dim, bg, TEXT_MIN),
                ("entry hovered", c.outline_dim, c.outline_hover, TEXT_MIN),
                ("current entry's bar", c.outline_bar, bg, MARK_MIN),
            ] {
                let got = contrast(fg, on);
                assert!(got >= min - 0.01, "{name}: {what} {got}");
            }
            assert_ne!(
                c.outline_current, c.outline_dim,
                "{name}: the current one stands out"
            );
        }
    }

    /// Every theme (M11ai Task 9): the narrow outline's panel (the overlay panel's fill) holds
    /// its current entry and the others to the text floor on the panel and on a hovered entry,
    /// the current entry's bar to the mark floor on the panel; the reading-progress line is the
    /// accent at the mark floor on the page.
    #[test]
    fn the_outline_panel_s_and_the_progress_line_s_colours_meet_their_floors() {
        use crate::ui_tokens::contrast;
        for (name, theme) in crate::theme::every_theme() {
            let c = Colours::of(&theme);
            let ui = theme.ui();
            let p = c.outline_panel;
            for (what, fg, on, min) in [
                ("panel entry current", p.current, ui.panel, TEXT_MIN),
                ("panel entry current hovered", p.current, p.hover, TEXT_MIN),
                ("panel entry", p.dim, ui.panel, TEXT_MIN),
                ("panel entry hovered", p.dim, p.hover, TEXT_MIN),
                ("panel bar", p.bar, ui.panel, MARK_MIN),
                ("progress line", c.progress, theme.background, MARK_MIN),
            ] {
                let got = contrast(fg, on);
                assert!(got >= min - 0.01, "{name}: {what} {got}");
            }
            assert_eq!(p.hover, ui.panel_hover, "{name}: the hover fill");
            assert_eq!(
                c.progress,
                ensure_contrast(ui.accent, theme.background, MARK_MIN),
                "{name}: the progress line is the accent"
            );
            assert_ne!(p.current, p.dim, "{name}: the current one stands out");
        }
    }

    /// Every theme (spec C): a stripe is the text at `STRIPE` over the page; the pills' fills
    /// show and the value's is a step stronger than the label's (further from the page, toward
    /// the text).
    #[test]
    fn stripes_and_pills_are_tints_of_the_text() {
        use crate::ui_tokens::contrast;
        use crate::ui_tokens::reader::STRIPE;
        for (name, theme) in crate::theme::every_theme() {
            let c = Colours::of(&theme);
            let bg = theme.background;
            assert_eq!(c.stripe, mix(bg, theme.ui().text, STRIPE), "{name}");
            assert_ne!(c.stripe, bg, "{name}: the stripe shows");
            let (label, value) = (contrast(c.pill_bg, bg), contrast(c.pill_value_bg, bg));
            assert!(label > 1.0, "{name}: the pill's fill shows ({label})");
            assert!(
                value > label,
                "{name}: the value's fill {value} not over {label}"
            );
        }
    }

    /// Body rows 2, 4, … (1, 3, … from 0) are striped; the 1st is not.
    #[test]
    fn every_second_body_row_is_striped() {
        let got: Vec<bool> = (0..5).map(striped).collect();
        assert_eq!(got, [false, true, false, true, false]);
    }

    #[test]
    fn the_front_matter_s_rest_reads_plus_n_more() {
        assert_eq!(more_label(5), "+ 5 more");
        assert_eq!(more_label(1), "+ 1 more");
    }

    /// In the reader, an image whose source is a badge's shows as its alt text (no brackets)
    /// with the badge look, still linked when it is; any other image keeps today's look, and
    /// chat keeps today's look for every image (`inline`).
    #[test]
    fn a_badge_in_a_sentence_has_the_badge_look_in_the_reader_only() {
        let spans = vec![
            t("CI "),
            Span::LinkedImage {
                alt: "build: passing".into(),
                src: "https://img.shields.io/badge/build-passing-green".into(),
                url: "https://example.com/ci".into(),
            },
            t(" and "),
            Span::Image {
                alt: "".into(),
                url: "https://example.com/docs/badge.svg".into(),
            },
            t(" "),
            Span::Image {
                alt: "logo".into(),
                url: "logo.png".into(),
            },
        ];
        let got = inline_with(&spans, true);
        assert_eq!(got.text, "CI build: passing and badge [logo]");
        assert_eq!(
            got.looks,
            vec![
                (3..17, Look::Badge),
                (22..27, Look::Badge),
                (28..34, Look::Image)
            ]
        );
        assert_eq!(
            got.links,
            vec![(3..17, "https://example.com/ci".to_string())]
        );
        let chat = inline(&spans);
        assert_eq!(chat.text, "CI [build: passing] and [] [logo]");
        assert!(chat.looks.iter().all(|(_, l)| *l != Look::Badge));
    }

    /// Every kind in every theme: a callout's title and body meet the text floor on its wash, its
    /// bar and icon the mark floor.
    #[test]
    fn callout_colours_meet_their_floors_in_every_theme() {
        use crate::ui_tokens::contrast;
        for (name, theme) in crate::theme::every_theme() {
            let colours = Colours::of(&theme);
            for kind in Callout::ALL {
                let c = colours.callout(kind);
                for (what, fg, min) in [
                    ("title", c.title, TEXT_MIN),
                    ("body", c.body, TEXT_MIN),
                    ("link", c.link, TEXT_MIN),
                    ("dim", c.dim, TEXT_MIN),
                    ("bar", c.bar, MARK_MIN),
                ] {
                    let got = contrast(fg, c.wash);
                    assert!(got >= min - 0.01, "{name}: {} {what} {got}", kind.title());
                }
                assert_ne!(c.wash, theme.background, "{name}: {} wash", kind.title());
            }
        }
    }

    /// The code header strip is a shade darker than the code's fill in every theme (spec A,
    /// "Code blocks"), dark ones too.
    #[test]
    fn the_code_header_is_darker_than_the_code_in_every_theme() {
        use crate::ui_tokens::relative_luminance;
        for (name, theme) in crate::theme::every_theme() {
            let c = Colours::of(&theme);
            let (header, code) = (
                relative_luminance(c.code_header_bg),
                relative_luminance(c.code_bg),
            );
            assert!(
                header < code,
                "{name}: header {header} not below code {code}"
            );
        }
    }

    /// A chat row's links get ids from its own base, so two rows' links never share one.
    #[test]
    fn column_link_ids_start_at_base() {
        let theme = crate::theme::every_theme()
            .into_iter()
            .next()
            .expect("a theme")
            .1;
        let draw = Draw {
            colours: Colours::of(&theme),
            code_font: "Menlo".into(),
            on_link: Rc::new(|_, _, _| {}),
            links: Cell::new(0),
            mermaid: Rc::new(Vec::new()),
            sizes: Sizes::CHAT,
            reader: None,
        };
        let blocks = crate::agent_markdown::parse(
            "See [the docs](https://e.x) and [more](https://f.x).\n\n- [a](https://g.x)",
        );
        let _ = column(&blocks, 3, &draw);
        assert!(draw.links.get() >= 3 << 20, "{}", draw.links.get());
        assert!(draw.links.get() < 4 << 20, "{}", draw.links.get());
        // Two linked blocks: two ids taken past the base.
        assert_eq!(draw.links.get(), (3 << 20) + 2);
    }

    #[test]
    fn markdown_is_md_or_markdown_in_any_case() {
        for yes in [
            "README.md",
            "a/b/Notes.MD",
            "x.Markdown",
            "/w/.github/c.markdown",
        ] {
            assert!(is_markdown(Path::new(yes)), "{yes}");
        }
        for no in ["main.rs", "md", "README", "a.mdx", "a.md.txt", "/w/md/"] {
            assert!(!is_markdown(Path::new(no)), "{no}");
        }
    }

    #[test]
    fn links_go_to_the_web_a_file_beside_it_a_heading_or_nowhere() {
        let base = Path::new("/w/docs");
        let web = |u: &str| LinkTarget::Web(u.into());
        assert_eq!(link_target(base, "https://e.x/a"), web("https://e.x/a"));
        assert_eq!(link_target(base, "http://e.x"), web("http://e.x"));
        assert_eq!(link_target(base, "mailto:a@e.x"), web("mailto:a@e.x"));
        let file = |p: &str, part: Option<&str>| {
            LinkTarget::File(PathBuf::from(p), part.map(str::to_string))
        };
        assert_eq!(
            link_target(base, "guide.md"),
            file("/w/docs/guide.md", None)
        );
        assert_eq!(
            link_target(base, "guide.md#install"),
            file("/w/docs/guide.md", Some("install"))
        );
        assert_eq!(
            link_target(base, "../src/main.rs#L3"),
            file("/w/docs/../src/main.rs", Some("L3"))
        );
        // An empty part is no part.
        assert_eq!(
            link_target(base, "guide.md#"),
            file("/w/docs/guide.md", None)
        );
        assert_eq!(link_target(base, "/etc/hosts"), file("/etc/hosts", None));
        assert_eq!(
            link_target(base, "my%20notes.md"),
            file("/w/docs/my notes.md", None)
        );
        assert_eq!(
            link_target(base, "#install"),
            LinkTarget::Here("install".into())
        );
        assert_eq!(
            link_target(base, " #caf%C3%A9 "),
            LinkTarget::Here("caf%C3%A9".into())
        );
        // A bare `#` (an empty part) goes nowhere.
        for nowhere in ["#", "", "javascript:alert(1)", "ftp://e.x/f", "file:///etc"] {
            assert_eq!(link_target(base, nowhere), LinkTarget::Nowhere, "{nowhere}");
        }
    }

    #[test]
    fn a_paragraph_is_one_string_with_its_looks_and_links() {
        let spans = vec![
            t("Hi\nthere "),
            Span::Bold("b".into()),
            t(" "),
            Span::Italic("i".into()),
            Span::Code("c".into()),
            Span::Link {
                text: "é".into(),
                url: "https://e.x".into(),
            },
            Span::Image {
                alt: "logo".into(),
                url: "a.png".into(),
            },
        ];
        let got = inline(&spans);
        assert_eq!(got.text, "Hi there b icé[logo]");
        assert_eq!(
            got.looks,
            vec![
                (9..10, Look::Bold),
                (11..12, Look::Italic),
                (12..13, Look::Code),
                (13..15, Look::Link(0)),
                (15..21, Look::Image),
            ]
        );
        assert_eq!(got.links, vec![(13..15, "https://e.x".to_string())]);
    }

    #[test]
    fn markers_by_kind_and_depth() {
        let b = Md::Bullet(vec![]);
        assert_eq!(marker(&b, 0).as_deref(), Some("•"));
        assert_eq!(marker(&b, 1).as_deref(), Some("◦"));
        assert_eq!(marker(&b, 2).as_deref(), Some("▪"));
        assert_eq!(marker(&b, 5).as_deref(), Some("▪"));
        assert_eq!(marker(&Md::Numbered(12, vec![]), 0).as_deref(), Some("12."));
        assert_eq!(marker(&Md::Task(false, vec![]), 0).as_deref(), Some("☐"));
        assert_eq!(marker(&Md::Task(true, vec![]), 3).as_deref(), Some("☑"));
        assert_eq!(marker(&Md::Para(vec![]), 0), None);
    }

    /// Agent chat draws as today (plan-time call 1): `Sizes::CHAT` is M11ac's numbers, and its
    /// gaps are M11ac's `gap_before`'s (its test, moved).
    #[test]
    fn chat_keeps_today_s_sizes() {
        assert_eq!(
            Sizes::CHAT,
            Sizes {
                body: 14.0,
                line: 1.5,
                headings: [24.0, 20.0, 17.0, 15.0, 14.0, 13.0],
                heading_line: 1.3,
                code: 13.0,
                code_line: 1.45,
                block_gap: 12.0,
                item_gap: 4.0,
                heading_above: 12.0,
                heading_below: 12.0,
                rule_below: 6.0,
                code_pad: (12.0, 10.0),
                code_radius: 6.0,
                indent: 20.0,
                marker_gap: 6.0,
                quote_pad: 12.0,
                quote_bar: 3.0,
                rule_margin: 4.0,
                rule_line: 1.0,
                table_cell: (10.0, 5.0),
            }
        );
        let chat = &Sizes::CHAT;
        let item = Md::Bullet(vec![]);
        let nested = Md::Nested(1, Box::new(Md::Task(false, vec![])));
        let para = Md::Para(vec![]);
        let heading = Md::Heading(2, vec![]);
        assert_eq!(gap_between(chat, None, &para), 0.0);
        assert_eq!(gap_between(chat, Some(&item), &nested), 4.0);
        assert_eq!(gap_between(chat, Some(&nested), &item), 4.0);
        assert_eq!(gap_between(chat, Some(&para), &item), 12.0);
        assert_eq!(gap_between(chat, Some(&item), &para), 12.0);
        assert_eq!(gap_between(chat, Some(&para), &para), 12.0);
        assert_eq!(gap_between(chat, Some(&para), &heading), 12.0);
        assert_eq!(gap_between(chat, Some(&heading), &para), 12.0);
    }

    /// The reader's spacing (spec A): 1 em between blocks, 32 pt above a heading and 12 under
    /// it, list items 6 apart, nothing over the first block.
    #[test]
    fn the_reader_s_gaps() {
        let r = &Sizes::reader();
        let para = Md::Para(vec![]);
        let heading = Md::Heading(2, vec![]);
        let item = Md::Bullet(vec![]);
        let nested = Md::Nested(1, Box::new(Md::Bullet(vec![])));
        assert_eq!(gap_between(r, Some(&para), &para), 16.0);
        assert_eq!(gap_between(r, Some(&para), &heading), 32.0);
        assert_eq!(gap_between(r, Some(&heading), &para), 12.0);
        assert_eq!(gap_between(r, Some(&item), &nested), 6.0);
        assert_eq!(gap_between(r, Some(&item), &item), 6.0);
        assert_eq!(gap_between(r, None, &para), 0.0);
        assert_eq!(gap_between(r, None, &heading), 0.0);
        assert_eq!(r.body, 16.0);
        assert_eq!(r.line, 1.65);
        assert_eq!(r.headings, [30.0, 24.0, 20.0, 17.0, 16.0, 15.0]);
        assert_eq!((r.code, r.code_line, r.rule_below), (14.0, 1.5, 8.0));
        assert_eq!((r.code_pad, r.code_radius), ((12.0, 10.0), 6.0));
    }

    /// Every size the reader draws with is a `ui_tokens::reader` token (spec: every size a named
    /// token), and today's values are kept: the list's indent and a number's gap, a quote's
    /// padding and bar, a rule's margin and line, a table cell's padding.
    #[test]
    fn the_reader_s_sizes_are_its_tokens() {
        use crate::ui_tokens::reader as t;
        let r = Sizes::reader();
        assert_eq!((r.indent, r.marker_gap), (t::LIST_INDENT, t::MARKER_GAP));
        assert_eq!((r.quote_pad, r.quote_bar), (t::QUOTE_PAD, t::QUOTE_BAR));
        assert_eq!((r.rule_margin, r.rule_line), (t::RULE_MARGIN, t::RULE_LINE));
        assert_eq!(r.table_cell, t::TABLE_CELL);
        assert_eq!((t::LIST_INDENT, t::MARKER_GAP), (20.0, 6.0));
        assert_eq!((t::QUOTE_PAD, t::QUOTE_BAR), (12.0, 3.0));
        assert_eq!((t::RULE_MARGIN, t::RULE_LINE), (4.0, 1.0));
        assert_eq!(t::CALLOUT_GAP, 6.0);
        assert_eq!(t::CODE_BORDER, 1.0);
        assert_eq!(
            (t::DIAGRAM_RADIUS, t::NOTE_GAP, t::NOTE_TEXT, t::NOTE_LINE),
            (6.0, 4.0, 12.0, 1.4)
        );
    }
}
