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
use crate::md_reader::{ReaderPlan, RowKind, fence_lang};
use crate::tab_bar::{Glyph, ICON, glyph};
use crate::terminal_view::hsla;
use crate::theme::{Theme, mix};
use crate::ui_tokens::reader::TABLE_COL_MIN;
use crate::ui_tokens::size::TERMINAL_PADDING;
use crate::ui_tokens::{QUIET_LINE_MIN, TEXT_MIN, ensure_contrast};

/// A list level's indent, and the marker's column.
const INDENT: f32 = 20.0;

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
    /// A path, from the file's own folder unless absolute (its `#part` dropped).
    File(PathBuf),
    /// A `#part` of this file, another scheme, or nothing: not followed.
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
    if path.is_empty() {
        return LinkTarget::Nowhere;
    }
    LinkTarget::File(base.join(percent_decoded(path)))
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
}

/// A paragraph's spans as one string: each styled piece's range and look, and each link's
/// range and address. A line break inside a paragraph is a space.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inline {
    pub text: String,
    pub looks: Vec<(Range<usize>, Look)>,
    pub links: Vec<(Range<usize>, String)>,
}

/// `spans` laid end to end (`Inline`).
pub fn inline(spans: &[Span]) -> Inline {
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
        Colours {
            text: ensure_contrast(ui.text, bg, TEXT_MIN),
            dim: ensure_contrast(ui.text_dim, bg, TEXT_MIN),
            link: ensure_contrast(ui.accent, bg, TEXT_MIN),
            code_bg,
            code_text: ensure_contrast(ui.text, code_bg, TEXT_MIN),
            line: ensure_contrast(ui.panel_border, bg, QUIET_LINE_MIN),
            header_bg: code_bg,
            header_text: ensure_contrast(ui.text, code_bg, TEXT_MIN),
            code_header_bg,
            code_header_text: ensure_contrast(ui.text_dim, code_header_bg, TEXT_MIN),
        }
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
    /// Each coloured code block's look, by block (missing: drawn uncoloured).
    pub code: Rc<HashMap<usize, Rc<CodeLook>>>,
    /// What shows "Copied" now.
    pub copied: Option<Copied>,
    /// A code block's Copy pressed (given its row).
    pub on_copy: OnCopy,
    /// The scenes' probe: each drawn row's content, as last prepainted (window points).
    #[cfg(feature = "ui-shots")]
    pub drawn: Drawn,
    /// The scenes' probe: each drawn code block's header.
    #[cfg(feature = "ui-shots")]
    pub code_drawn: CodeDrawn,
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
    #[allow(dead_code)] // Task 7 copies a heading's link
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
    let (gap, content) = match doc.plan.row_kind(row) {
        // Task 6 draws the front matter.
        RowKind::Front => (0.0, div().into_any_element()),
        RowKind::Block(b) => {
            let Some(md) = doc.blocks.get(b) else {
                return div().into_any_element();
            };
            let gap = match b.checked_sub(1) {
                Some(prev) => gap_between(s, doc.blocks.get(prev), md),
                None => gap_after(s, None, md),
            };
            let content = match (draw.mermaid.get(b), md) {
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
                _ => prose(block(md, draw)),
            };
            (gap, content)
        }
    };
    let (side, end) = TERMINAL_PADDING;
    let top = if row == 0 { end } else { gap };
    let bottom = if row + 1 == rows { end } else { 0.0 };
    // The reader's area (the outline, when it shows, will take its share of the row), its
    // content centred along a row: the content's width is settled before its height is
    // measured (centred in a column, taffy measured a paragraph's height at the area's width).
    let area = div().w_full().flex().flex_row().justify_center();
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
        .px(px(side))
        .pt(px(top))
        .pb(px(bottom))
        .text_size(px(s.body))
        .line_height(relative(s.line))
        .text_color(hsla(draw.colours.text))
        .child(area.child(content))
        .into_any_element()
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

/// A code block in the reader (M11ai): a header strip on its top edge (a shade off the code's
/// fill: the fence's language on the left, as the fence names it, nothing when it names none;
/// Copy on the right, dim, brighter under the pointer; "Copied" with a tick while `copied` says
/// so), then its code: coloured once the colour job's answer for block `b` is in, else in the
/// code colour. The button's element id is from `row`.
fn reader_code(row: usize, b: usize, md: &Md, draw: &Draw, reader: &ReaderDraw) -> AnyElement {
    use crate::ui_tokens::reader::{CODE_HEADER, CODE_HEADER_TEXT, COPY_GAP};
    let Md::Code { lang, text: code } = md else {
        return block(md, draw);
    };
    let (c, s) = (draw.colours, &draw.sizes);
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
    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .h(px(CODE_HEADER))
        .px(px(s.code_pad.0))
        // Inside the block's 1 pt border: its corners 1 pt less round.
        .rounded_t(px((s.code_radius - 1.0).max(0.0)))
        .bg(hsla(c.code_header_bg))
        .text_size(px(CODE_HEADER_TEXT))
        .text_color(hsla(dim))
        .child(div().children(lang))
        .child(button);
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
    div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(s.code_radius))
        .bg(hsla(c.code_bg))
        .border_1()
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
                    .child(img(image.clone()).size_full().rounded(px(6.0))),
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
                        .mt(px(4.0))
                        .text_size(px(12.0))
                        .line_height(relative(1.4))
                        .text_color(hsla(draw.colours.dim))
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
    let (c, s) = (draw.colours, &draw.sizes);
    match md {
        Md::Para(spans) => text(spans, draw),
        Md::Heading(level, spans) => {
            let size = s.headings[(*level as usize).clamp(1, 6) - 1];
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
            .pl(px(12.0))
            .border_l(px(3.0))
            .border_color(hsla(c.line))
            .text_color(hsla(c.dim))
            .children(blocks_of(inner, draw))
            .into_any_element(),
        Md::Table { align, head, rows } => table(align, head, rows, None, draw),
        Md::Rule => div()
            .w_full()
            .my(px(4.0))
            .h(px(1.0))
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
    div()
        .w_full()
        .flex()
        .flex_row()
        .pl(px(depth as f32 * INDENT))
        .child(
            div()
                .flex_none()
                .min_w(px(INDENT))
                .pr(px(if numbered { 6.0 } else { 0.0 }))
                .text_color(hsla(draw.colours.dim))
                .children(marker(md, depth)),
        )
        .child(div().flex_1().min_w(px(0.0)).child(text(spans, draw)))
        .into_any_element()
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
    let c = draw.colours;
    let n = head.len().max(1);
    let cell = |spans: &[Span], col: usize, row: usize, header: bool| {
        let d = div()
            .min_w(px(0.0))
            .px(px(10.0))
            .py(px(5.0))
            .when(col + 1 < n, |d| d.border_r_1())
            .when(row > 0, |d| d.border_t_1())
            .border_color(hsla(c.line))
            .when(header, |d| {
                d.bg(hsla(c.header_bg))
                    .text_color(hsla(c.header_text))
                    .font_weight(FontWeight::SEMIBOLD)
            });
        let d = match align.get(col) {
            Some(Align::Center) => d.text_center(),
            Some(Align::Right) => d.text_right(),
            _ => d,
        };
        d.child(text(spans, draw)).into_any_element()
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
        .rounded(px(4.0))
        .overflow_hidden()
        .children(cells)
        .into_any_element()
}

/// A paragraph's spans as one wrapping text: bold, italic, code in the terminal's font on its
/// fill, links in the link colour, underlined and clickable, images' alt text dim.
fn text(spans: &[Span], draw: &Draw) -> AnyElement {
    let c = draw.colours;
    let laid = inline(spans);
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
                color: Some(hsla(c.link)),
                underline: Some(UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(hsla(c.link)),
                    wavy: false,
                }),
                ..Default::default()
            },
            Look::Image => HighlightStyle {
                color: Some(hsla(c.dim)),
                font_style: Some(FontStyle::Italic),
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
            ] {
                let got = contrast(fg, on);
                assert!(got >= TEXT_MIN - 0.01, "{name}: {what} {got}");
            }
            // The code header is a shade off the code's fill.
            assert_ne!(c.code_header_bg, c.code_bg, "{name}");
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
    fn links_go_to_the_web_a_file_beside_it_or_nowhere() {
        let base = Path::new("/w/docs");
        let web = |u: &str| LinkTarget::Web(u.into());
        assert_eq!(link_target(base, "https://e.x/a"), web("https://e.x/a"));
        assert_eq!(link_target(base, "http://e.x"), web("http://e.x"));
        assert_eq!(link_target(base, "mailto:a@e.x"), web("mailto:a@e.x"));
        let file = |p: &str| LinkTarget::File(PathBuf::from(p));
        assert_eq!(link_target(base, "guide.md"), file("/w/docs/guide.md"));
        assert_eq!(
            link_target(base, "../src/main.rs#L3"),
            file("/w/docs/../src/main.rs")
        );
        assert_eq!(link_target(base, "/etc/hosts"), file("/etc/hosts"));
        assert_eq!(
            link_target(base, "my%20notes.md"),
            file("/w/docs/my notes.md")
        );
        for nowhere in [
            "#install",
            "",
            "javascript:alert(1)",
            "ftp://e.x/f",
            "file:///etc",
        ] {
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
}
