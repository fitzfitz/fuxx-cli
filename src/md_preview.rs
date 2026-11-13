//! The file tab's Markdown Preview (M11ac): a `.md` or `.markdown` file shown rendered, read-only,
//! instead of its source. The file is parsed with `agent_markdown::parse` (again only when the
//! tab reads a new version) and drawn as GPUI elements: a block per `Md`, scrolled by GPUI (its
//! `ScrollHandle` kept by the tab, so a reload keeps the place as near as the new text allows).
//! Prose is in the UI font, wrapped to the tab's width; code (blocks and inline) in the
//! terminal's font on the panel colour. Links are clickable: a web or mail link opens in the
//! default browser, one to a file next to it opens that file in a tab; anything else does
//! nothing. Colours come from the theme (`UiColors`), the text held to `TEXT_MIN`.
//!
//! `is_markdown`, `link_target`, `inline`, `marker` and `gap_before` are pure and tested.

use std::cell::Cell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, App, FontStyle, FontWeight, HighlightStyle, InteractiveText, SharedString,
    StyledText, UnderlineStyle, Window, div, prelude::*, px, relative,
};

use crate::agent_markdown::{Align, Md, Span};
use crate::file_tab::expand_tabs;
use crate::terminal_view::hsla;
use crate::theme::{Theme, mix};
use crate::ui_tokens::{QUIET_LINE_MIN, TEXT_MIN, ensure_contrast};

/// Prose size, and the headings' by level (1–6).
pub const BODY: f32 = 14.0;
const HEADINGS: [f32; 6] = [24.0, 20.0, 17.0, 15.0, 14.0, 13.0];
/// Code (blocks and inline), in the terminal's font.
const CODE: f32 = 13.0;
/// A list level's indent, and the marker's column.
const INDENT: f32 = 20.0;
/// The reader's column: padding round it, and its widest (long lines read badly).
const PAD_X: f32 = 24.0;
const PAD_Y: f32 = 16.0;
pub const COLUMN_MAX: f32 = 860.0;

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
/// items of one list, more between other blocks.
pub fn gap_before(prev: Option<&Md>, cur: &Md) -> f32 {
    match prev {
        None => 0.0,
        Some(prev) if is_item(prev) && is_item(cur) => ITEM_GAP,
        Some(_) => BLOCK_GAP,
    }
}

/// Between items of one list, and between other blocks.
const ITEM_GAP: f32 = 4.0;
const BLOCK_GAP: f32 = 12.0;

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
    pub header_bg: Rgb,
}

impl Colours {
    pub fn of(theme: &Theme) -> Colours {
        let ui = theme.ui();
        let bg = theme.background;
        // A tint of the text over the page: it shows in a light theme too (the panel colour
        // is all but the page there).
        let code_bg = mix(bg, ui.text, 0.07);
        Colours {
            text: ensure_contrast(ui.text, bg, TEXT_MIN),
            dim: ensure_contrast(ui.text_dim, bg, TEXT_MIN),
            link: ensure_contrast(ui.accent, bg, TEXT_MIN),
            code_bg,
            code_text: ensure_contrast(ui.text, code_bg, TEXT_MIN),
            line: ensure_contrast(ui.panel_border, bg, QUIET_LINE_MIN),
            header_bg: code_bg,
        }
    }
}

/// What a link click does, given its address.
pub type OnLink = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// What drawing needs besides the blocks: the colours, the code font, what a link click does,
/// and a count for the links' element ids.
pub struct Draw {
    pub colours: Colours,
    pub code_font: SharedString,
    pub on_link: OnLink,
    pub links: Cell<usize>,
}

/// The reader's column: `blocks` drawn one under another, padded, at most `COLUMN_MAX` wide.
pub fn column(blocks: &[Md], draw: &Draw) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(COLUMN_MAX + 2.0 * PAD_X))
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .text_size(px(BODY))
        .line_height(relative(1.5))
        .text_color(hsla(draw.colours.text))
        .children(blocks_of(blocks, draw))
        .into_any_element()
}

fn blocks_of(blocks: &[Md], draw: &Draw) -> Vec<AnyElement> {
    let mut prev = None;
    blocks
        .iter()
        .map(|md| {
            let gap = gap_before(prev, md);
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
    let c = draw.colours;
    match md {
        Md::Para(spans) => text(spans, draw),
        Md::Heading(level, spans) => {
            let size = HEADINGS[(*level as usize).clamp(1, 6) - 1];
            div()
                .w_full()
                .text_size(px(size))
                .line_height(relative(1.3))
                .font_weight(FontWeight::SEMIBOLD)
                .when(*level <= 2, |d| {
                    d.pb(px(6.0)).border_b_1().border_color(hsla(c.line))
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
                .px(px(12.0))
                .py(px(10.0))
                .rounded(px(6.0))
                .bg(hsla(c.code_bg))
                .border_1()
                .border_color(hsla(c.line))
                .font_family(draw.code_font.clone())
                .text_size(px(CODE))
                .line_height(relative(1.45))
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
        Md::Table { align, head, rows } => table(align, head, rows, draw),
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

/// A table: a grid of equal columns, the header filled and bold, lines between the cells.
fn table(align: &[Align], head: &[Vec<Span>], rows: &[Vec<Vec<Span>>], draw: &Draw) -> AnyElement {
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
                d.bg(hsla(c.header_bg)).font_weight(FontWeight::SEMIBOLD)
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
            ] {
                let got = contrast(fg, on);
                assert!(got >= TEXT_MIN - 0.01, "{name}: {what} {got}");
            }
        }
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

    #[test]
    fn list_items_sit_close_and_other_blocks_apart() {
        let item = Md::Bullet(vec![]);
        let nested = Md::Nested(1, Box::new(Md::Task(false, vec![])));
        let para = Md::Para(vec![]);
        assert_eq!(gap_before(None, &para), 0.0);
        assert!(gap_before(Some(&item), &nested) < gap_before(Some(&para), &item));
        assert_eq!(
            gap_before(Some(&item), &nested),
            gap_before(Some(&nested), &item)
        );
        assert_eq!(
            gap_before(Some(&item), &para),
            gap_before(Some(&para), &para)
        );
    }
}
