//! Link detection under a grid point: OSC 8 hyperlinks first, then plain-text URLs. Command
//! block tags (`fuxx-block:` hyperlinks, M11l) are never links: they are not shown, hovered
//! or opened, and the plain-text search still runs on their cell.
//!
//! Security boundary: only `http://`, `https://` and `mailto:` are ever returned (`allowed`),
//! so program output can never make fuxx open `file://` or other schemes. Plain-text matching
//! and punctuation trimming mirror Zed's `crates/terminal/src/alacritty/hyperlinks.rs`.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Boundary, Direction, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::search::{RegexIter, RegexSearch};

use crate::blocks::URI_PREFIX;

/// Zed's `URL_REGEX`, narrowed to the allowed schemes.
const URL_REGEX: &str =
    "(https://|http://|mailto:)[^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`']+";

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub url: String,
    pub start: Point,
    pub end: Point,
}

/// Only these schemes may ever be opened (case-insensitive).
pub fn allowed(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
}

pub struct LinkFinder {
    regex: RegexSearch,
}

impl LinkFinder {
    pub fn new() -> Self {
        Self {
            regex: RegexSearch::new(URL_REGEX).expect("URL regex is valid"),
        }
    }

    /// The link under `point`: an OSC 8 hyperlink if the cell has one, else a plain-text URL.
    /// Returns `None` unless the URL's scheme is `allowed`.
    pub fn find<L: EventListener>(&mut self, term: &Term<L>, point: Point) -> Option<Link> {
        let link = Self::osc8(term, point).or_else(|| self.plain(term, point))?;
        allowed(&link.url).then_some(link)
    }

    /// The cell's OSC 8 link. A command block's tag (M11l) is not one: the cell is read as
    /// plain text.
    fn osc8<L: EventListener>(term: &Term<L>, point: Point) -> Option<Link> {
        let uri = term.grid()[point].hyperlink()?.uri().to_string();
        if uri.starts_with(URI_PREFIX) {
            return None;
        }
        let same = |col: usize| {
            term.grid()[Point::new(point.line, alacritty_terminal::index::Column(col))]
                .hyperlink()
                .is_some_and(|h| h.uri() == uri)
        };
        let mut start = point.column.0;
        while start > 0 && same(start - 1) {
            start -= 1;
        }
        let mut end = point.column.0;
        while end + 1 < term.columns() && same(end + 1) {
            end += 1;
        }
        Some(Link {
            url: uri,
            start: Point::new(point.line, alacritty_terminal::index::Column(start)),
            end: Point::new(point.line, alacritty_terminal::index::Column(end)),
        })
    }

    fn plain<L: EventListener>(&mut self, term: &Term<L>, point: Point) -> Option<Link> {
        let found = RegexIter::new(
            term.line_search_left(point),
            term.line_search_right(point),
            Direction::Right,
            term,
            &mut self.regex,
        )
        .find(|m| m.contains(&point))?;
        let (start, mut end) = (*found.start(), *found.end());
        let text = term.bounds_to_string(start, end);
        let url = trim_url(&text);
        if url.is_empty() {
            return None;
        }
        let trimmed = text.chars().count() - url.chars().count();
        if trimmed > 0 {
            end = end.sub(term, Boundary::Grid, trimmed);
        }
        // The point may sit on trimmed punctuation, which is not part of the link.
        if point > end {
            return None;
        }
        Some(Link {
            url: url.to_string(),
            start,
            end,
        })
    }
}

/// Trim trailing `. , : ; (` and unbalanced `)` (Zed's `sanitize_url_punctuation`).
fn trim_url(text: &str) -> &str {
    let mut url = text;
    loop {
        let Some(last) = url.chars().last() else {
            return url;
        };
        let strip = match last {
            '.' | ',' | ':' | ';' | '(' => true,
            ')' => url.matches(')').count() > url.matches('(').count(),
            _ => false,
        };
        if !strip {
            return url;
        }
        url = &url[..url.len() - last.len_utf8()];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TermDims;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    fn term_with(bytes: &[u8], cols: usize) -> Term<VoidListener> {
        let mut term = Term::new(Config::default(), &TermDims { cols, rows: 3 }, VoidListener);
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        processor.advance(&mut term, bytes);
        term
    }

    #[test]
    fn url_under_point_is_found_and_trimmed() {
        let term = term_with(b"see https://example.com/a). ok", 40);
        let link = LinkFinder::new()
            .find(&term, Point::new(Line(0), Column(8)))
            .unwrap();
        assert_eq!(link.url, "https://example.com/a");
        assert_eq!((link.start.column.0, link.end.column.0), (4, 24));
    }

    #[test]
    fn osc8_link_is_found() {
        let term = term_with(b"\x1b]8;;https://x.dev\x1b\\link\x1b]8;;\x1b\\ plain", 40);
        let link = LinkFinder::new()
            .find(&term, Point::new(Line(0), Column(1)))
            .unwrap();
        assert_eq!(
            (link.url.as_str(), link.start.column.0, link.end.column.0),
            ("https://x.dev", 0, 3)
        );
    }

    #[test]
    fn disallowed_schemes_never_open() {
        let term = term_with(
            b"\x1b]8;;file:///Applications/Calculator.app\x1b\\calc\x1b]8;;\x1b\\ file:///etc/passwd",
            60,
        );
        let mut finder = LinkFinder::new();
        assert_eq!(finder.find(&term, Point::new(Line(0), Column(1))), None);
        assert_eq!(finder.find(&term, Point::new(Line(0), Column(10))), None);
        assert!(
            allowed("HTTPS://a.b")
                && allowed("mailto:x@y.z")
                && !allowed("javascript:x")
                && !allowed("ssh://h")
        );
    }

    #[test]
    fn plain_text_has_no_link() {
        assert_eq!(
            LinkFinder::new().find(
                &term_with(b"just words", 20),
                Point::new(Line(0), Column(2))
            ),
            None
        );
    }

    #[test]
    fn block_tags_are_not_links() {
        // A command block's tag on column 0 of a row whose text is a URL (M11l).
        let term = term_with(
            b"\x1b]8;id=fuxx-block-1;fuxx-block:?v=1&s=done&p=0&cmd=https%3A%2F%2Fx.io\x1b\\h\x1b]8;;\x1b\\ttps://x.io",
            40,
        );
        assert!(term.grid()[Line(0)][Column(0)].hyperlink().is_some());
        let mut finder = LinkFinder::new();
        for col in [0, 3] {
            let link = finder.find(&term, Point::new(Line(0), Column(col)));
            assert_eq!(
                link,
                Some(Link {
                    url: "https://x.io".into(),
                    start: Point::new(Line(0), Column(0)),
                    end: Point::new(Line(0), Column(11)),
                }),
                "column {col}"
            );
        }
        // A tag on plain text is no link at all.
        let term = term_with(
            b"\x1b]8;id=fuxx-block-2;fuxx-block:?v=1&s=prompt&p=0\x1b\\$\x1b]8;;\x1b\\ ls",
            40,
        );
        assert_eq!(finder.find(&term, Point::new(Line(0), Column(0))), None);
    }
}
