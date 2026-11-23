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

use std::path::{Path, PathBuf};

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

/// A file path in a terminal's output (batch 3, chore 6), maybe with a line and a column
/// (`src/main.rs:42:7`): a path with a `/` or a name with an extension.
const PATH_REGEX: &str = "[~.A-Za-z0-9_+@-]*(/[A-Za-z0-9_.+@-]+)+(:[0-9]+){0,2}|[A-Za-z0-9_+@-][A-Za-z0-9_.+@-]*[.][A-Za-z0-9]+(:[0-9]+){0,2}";

/// A path someone might ⌘-click: its text, and its line and column if it gave them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathRef {
    pub path: String,
    pub line: Option<usize>,
    pub col: Option<usize>,
}

/// `text` (as matched in the output) as a path with its line and column: trailing punctuation
/// dropped (`src/a.rs:4:` from a compiler, `(src/a.rs)` in a sentence); none when nothing of
/// a path is left.
pub fn path_ref(text: &str) -> Option<PathRef> {
    let text = trim_url(text.trim_start_matches(['(', '[', '<', '"', '\'']));
    let text = text.trim_end_matches([')', ']', '>', '"', '\'']);
    let mut parts = text.splitn(3, ':');
    let path = parts.next()?.trim_end_matches('.');
    let number = |p: Option<&str>| p.and_then(|n| n.parse::<usize>().ok());
    let (line, col) = (number(parts.next()), number(parts.next()));
    let named = path.chars().any(|c| c.is_ascii_alphanumeric());
    named.then(|| PathRef {
        path: path.to_string(),
        line,
        col: col.filter(|_| line.is_some()),
    })
}

/// Where path `text` points from folder `cwd`: `~/` is the home folder, an absolute path is
/// itself, anything else is under `cwd`. None for a relative path with no folder known.
pub fn resolve_path(text: &str, cwd: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(rest) = text.strip_prefix("~/") {
        return Some(home?.join(rest));
    }
    let path = Path::new(text);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    Some(cwd?.join(path))
}

/// Finds the file path under a grid point (plain text only: OSC 8 links are `LinkFinder`'s).
pub struct PathFinder {
    regex: RegexSearch,
}

impl PathFinder {
    pub fn new() -> Self {
        Self {
            regex: RegexSearch::new(PATH_REGEX).expect("path regex is valid"),
        }
    }

    /// The path-looking text under `point`, its trailing punctuation dropped; `url` is that
    /// text (`path_ref` reads it). Whether the file exists is the caller's to check.
    pub fn find<L: EventListener>(&mut self, term: &Term<L>, point: Point) -> Option<Link> {
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
        let kept = trim_url(&text);
        path_ref(kept)?;
        let trimmed = text.chars().count() - kept.chars().count();
        if trimmed > 0 {
            end = end.sub(term, Boundary::Grid, trimmed);
        }
        if point > end {
            return None;
        }
        Some(Link {
            url: kept.to_string(),
            start,
            end,
        })
    }
}

/// Trim trailing `. , : ; (` and unbalanced `)` (Zed's `sanitize_url_punctuation`).
pub(crate) fn trim_url(text: &str) -> &str {
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
    fn a_path_keeps_its_line_and_column_and_loses_trailing_punctuation() {
        let r = |path: &str, line, col| {
            Some(PathRef {
                path: path.into(),
                line,
                col,
            })
        };
        assert_eq!(path_ref("src/main.rs:42"), r("src/main.rs", Some(42), None));
        assert_eq!(
            path_ref("src/main.rs:42:7:"),
            r("src/main.rs", Some(42), Some(7))
        );
        assert_eq!(path_ref("./a/b.txt."), r("./a/b.txt", None, None));
        assert_eq!(path_ref("main.rs,"), r("main.rs", None, None));
        assert_eq!(path_ref("(src/lib.rs)"), r("src/lib.rs", None, None));
        assert_eq!(path_ref("..."), None);
        assert_eq!(path_ref(""), None);
    }

    #[test]
    fn a_path_resolves_against_the_folder_and_home() {
        let cwd = Path::new("/code/my-app");
        let home = Path::new("/Users/me");
        assert_eq!(
            resolve_path("src/main.rs", Some(cwd), Some(home)),
            Some(PathBuf::from("/code/my-app/src/main.rs"))
        );
        assert_eq!(
            resolve_path("/etc/hosts", None, None),
            Some(PathBuf::from("/etc/hosts"))
        );
        assert_eq!(
            resolve_path("~/notes.md", Some(cwd), Some(home)),
            Some(PathBuf::from("/Users/me/notes.md"))
        );
        assert_eq!(resolve_path("src/main.rs", None, Some(home)), None);
    }

    #[test]
    fn a_path_in_the_output_is_found_under_the_point() {
        let term = term_with(b"error at src/main.rs:42:7: oops", 40);
        let mut finder = PathFinder::new();
        let link = finder.find(&term, Point::new(Line(0), Column(12))).unwrap();
        assert_eq!(link.url, "src/main.rs:42:7");
        assert_eq!((link.start.column.0, link.end.column.0), (9, 24));
        assert_eq!(finder.find(&term, Point::new(Line(0), Column(1))), None);
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
