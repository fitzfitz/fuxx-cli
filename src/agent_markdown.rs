#![allow(dead_code)] // consumers arrive in M11aa Tasks 7–11; remove when wired
//! The small markdown subset agent messages use (M11aa): paragraphs, headings, bullets, numbered
//! items, fenced code, and inline code / bold / links. Pure and GPUI-free. Anything it does not
//! understand stays literal text, and an unterminated fence is code to the end (streaming).
//!
//! M11ac extends it, additively, for the file tab's Markdown Preview: italic, images (their alt
//! text), autolinks, `\` escapes, block quotes (holding blocks), tables, horizontal rules,
//! setext headings, task items, and list items nested by their indent (`Md::Nested`). An item's
//! indented next line joins it. Not understood (left as text): HTML, indented code blocks,
//! reference links, footnotes, strikethrough, a span inside bold or italic.

#[derive(Clone, Debug, PartialEq)]
pub enum Md {
    Para(Vec<Span>),
    Heading(u8, Vec<Span>),
    Bullet(Vec<Span>),
    Numbered(u32, Vec<Span>),
    Code {
        lang: Option<String>,
        text: String,
    },
    /// M11ac (a whole file read as Markdown, the file tab's Preview): a task item, done or not.
    Task(bool, Vec<Span>),
    /// A list item (`Bullet`, `Numbered`, `Task`) this many levels in (1 and up).
    Nested(u8, Box<Md>),
    /// A block quote: the blocks inside it.
    Quote(Vec<Md>),
    /// A table: each column's alignment, the header's cells, and each row's (as many cells).
    Table {
        align: Vec<Align>,
        head: Vec<Vec<Span>>,
        rows: Vec<Vec<Vec<Span>>>,
    },
    /// A horizontal rule.
    Rule,
}

/// A table column's alignment (`:--`, `:-:`, `--:`; none given: `Default`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Default,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Span {
    Text(String),
    Code(String),
    Bold(String),
    Link {
        text: String,
        url: String,
    },
    /// M11ac: `*text*` or `_text_`.
    Italic(String),
    /// M11ac: `![alt](url)` (shown as its alt text).
    Image {
        alt: String,
        url: String,
    },
}

/// The deepest quotes nest (review fix): a deeper `>` is text, so parsing (and drawing) a
/// crafted file can't recurse without end.
pub const QUOTE_DEPTH: usize = 16;

/// What a list item is.
#[derive(Clone, Copy)]
enum Item {
    Bullet,
    Numbered(u32),
    Task(bool),
}

/// Parse message text into blocks. (M11ac: also a whole file read as Markdown: block quotes,
/// tables, rules, setext headings, task items and nested lists; the agent subset parses as
/// before.)
pub fn parse(text: &str) -> Vec<Md> {
    parse_in(text, 0)
}

/// `parse`, for the text inside `quotes` block quotes.
fn parse_in(text: &str, quotes: usize) -> Vec<Md> {
    let lines: Vec<&str> = text.lines().collect();
    let mut p = Parser {
        quotes,
        ..Parser::default()
    };
    let mut i = 0;
    while i < lines.len() {
        i = p.line(&lines, i);
    }
    p.flush();
    p.out
}

#[derive(Default)]
struct Parser<'a> {
    out: Vec<Md>,
    para: Vec<&'a str>,
    /// The list item being read: its depth, kind and lines (an indented line joins it).
    item: Option<(u8, Item, Vec<&'a str>)>,
    /// The open list items' indents, outermost first: an item's depth is its place here.
    indents: Vec<usize>,
    /// How many quotes this text is in (at `QUOTE_DEPTH`, a `>` is text).
    quotes: usize,
}

impl<'a> Parser<'a> {
    /// Read the line at `i` (and any after it that belong to the same block); the next one's
    /// index.
    fn line(&mut self, lines: &[&'a str], i: usize) -> usize {
        let line = lines[i];
        let trimmed = line.trim_start();
        let indent = indent_of(line);
        if trimmed.trim().is_empty() {
            self.flush();
            return i + 1;
        }
        if let Some(rest) = trimmed.strip_prefix("```") {
            self.block(indent);
            let lang = rest.trim();
            let mut body: Vec<&str> = Vec::new();
            let mut at = i + 1;
            while at < lines.len() {
                let l = lines[at];
                at += 1;
                if l.trim_start().starts_with("```") {
                    break;
                }
                body.push(l);
            }
            self.out.push(Md::Code {
                lang: (!lang.is_empty()).then(|| lang.to_string()),
                text: body.join("\n"),
            });
            return at;
        }
        if trimmed.starts_with('>') && self.quotes < QUOTE_DEPTH {
            self.block(indent);
            let mut inner: Vec<&str> = Vec::new();
            let mut at = i;
            while let Some(rest) = lines.get(at).and_then(|l| l.trim_start().strip_prefix('>')) {
                inner.push(rest.strip_prefix(' ').unwrap_or(rest));
                at += 1;
            }
            self.out
                .push(Md::Quote(parse_in(&inner.join("\n"), self.quotes + 1)));
            return at;
        }
        if let Some(align) = lines.get(i + 1).and_then(|next| separator(next))
            && trimmed.contains('|')
            && cells(trimmed).len() == align.len()
        {
            self.block(indent);
            let head: Vec<Vec<Span>> = cells(trimmed).into_iter().map(spans).collect();
            let width = head.len();
            let mut rows = Vec::new();
            let mut at = i + 2;
            while let Some(l) = lines
                .get(at)
                .filter(|l| !l.trim().is_empty() && l.contains('|'))
            {
                let mut row: Vec<Vec<Span>> = cells(l.trim()).into_iter().map(spans).collect();
                row.resize(width, Vec::new());
                rows.push(row);
                at += 1;
            }
            self.out.push(Md::Table { align, head, rows });
            return at;
        }
        if !self.para.is_empty() {
            let bare = line.trim();
            let all = |c: char| bare.chars().all(|x| x == c);
            let level = if all('=') && !bare.is_empty() {
                Some(1)
            } else if all('-') && bare.len() >= 3 {
                Some(2)
            } else {
                None
            };
            if let Some(level) = level {
                let title = spans(&self.para.join("\n"));
                self.para.clear();
                self.out.push(Md::Heading(level, title));
                self.indents.clear();
                return i + 1;
            }
        }
        if is_rule(trimmed) {
            self.block(indent);
            self.out.push(Md::Rule);
        } else if let Some((level, rest)) = heading(trimmed) {
            self.block(indent);
            self.out.push(Md::Heading(level, spans(rest)));
        } else if let Some((kind, rest)) = list_item(trimmed) {
            self.flush();
            while self.indents.last().is_some_and(|&top| top > indent) {
                self.indents.pop();
            }
            if self.indents.last() != Some(&indent) {
                self.indents.push(indent);
            }
            let depth = (self.indents.len() - 1).min(u8::MAX as usize) as u8;
            self.item = Some((depth, kind, vec![rest.trim()]));
        } else if let Some((_, _, item_lines)) = self.item.as_mut().filter(|_| indent >= 2) {
            item_lines.push(line.trim());
        } else {
            self.flush_item();
            if self.para.is_empty() && indent == 0 {
                self.indents.clear();
            }
            self.para.push(line.trim());
        }
        i + 1
    }

    /// A block that isn't a list item starts: what was being read ends, and at the left edge,
    /// so do the lists.
    fn block(&mut self, indent: usize) {
        self.flush();
        if indent == 0 {
            self.indents.clear();
        }
    }

    fn flush(&mut self) {
        self.flush_item();
        if !self.para.is_empty() {
            self.out.push(Md::Para(spans(&self.para.join("\n"))));
            self.para.clear();
        }
    }

    fn flush_item(&mut self) {
        let Some((depth, kind, lines)) = self.item.take() else {
            return;
        };
        let text = spans(&lines.join("\n"));
        let md = match kind {
            Item::Bullet => Md::Bullet(text),
            Item::Numbered(n) => Md::Numbered(n, text),
            Item::Task(done) => Md::Task(done, text),
        };
        self.out.push(match depth {
            0 => md,
            d => Md::Nested(d, Box::new(md)),
        });
    }
}

/// A line's indent in columns (a tab: 4).
fn indent_of(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// A list item's kind and text: `- `, `* `, `+ `, `1. `; a bullet's `[ ] ` or `[x] ` makes it
/// a task.
fn list_item(line: &str) -> Option<(Item, &str)> {
    if let Some(rest) = ["- ", "* ", "+ "].iter().find_map(|m| line.strip_prefix(m)) {
        let rest = rest.trim_start();
        for (mark, done) in [("[ ]", false), ("[x]", true), ("[X]", true)] {
            if let Some(after) = rest.strip_prefix(mark)
                && (after.is_empty() || after.starts_with(' '))
            {
                return Some((Item::Task(done), after));
            }
        }
        return Some((Item::Bullet, rest));
    }
    if line == "-" || line == "*" || line == "+" {
        return Some((Item::Bullet, ""));
    }
    numbered(line).map(|(n, rest)| (Item::Numbered(n), rest))
}

/// `---`, `***` or `___` (three or more, spaces allowed between).
fn is_rule(line: &str) -> bool {
    let marks: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
    marks.len() >= 3 && matches!(marks[0], '-' | '*' | '_') && marks.iter().all(|&c| c == marks[0])
}

/// A table's separator line (`|:--|:-:|--:|`): each column's alignment.
fn separator(line: &str) -> Option<Vec<Align>> {
    let line = line.trim();
    if !line.contains('-') {
        return None;
    }
    cells(line)
        .into_iter()
        .map(|cell| {
            let left = cell.starts_with(':');
            let right = cell.ends_with(':');
            let dashes = cell.trim_start_matches(':').trim_end_matches(':');
            if dashes.is_empty() || !dashes.chars().all(|c| c == '-') {
                return None;
            }
            Some(match (left, right) {
                (true, true) => Align::Center,
                (true, false) => Align::Left,
                (false, true) => Align::Right,
                (false, false) => Align::Default,
            })
        })
        .collect()
}

/// A table line's cells, trimmed: split at each `|` that isn't escaped or in inline code, the
/// first and last dropped when the line starts or ends with one.
fn cells(line: &str) -> Vec<&str> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = match line.strip_suffix('|') {
        Some(rest) if !rest.ends_with('\\') => rest,
        _ => line,
    };
    let mut out = Vec::new();
    let (mut start, mut code, mut escaped) = (0, false, false);
    for (i, c) in line.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '`' => code = !code,
            '|' if !code => {
                out.push(line[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(line[start..].trim());
    out
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) {
        let rest = line[hashes..].strip_prefix(' ')?.trim();
        // Closing hashes (`## Title ##`) go.
        let open = rest.trim_end_matches('#');
        if open.len() < rest.len() && (open.is_empty() || open.ends_with(' ')) {
            return Some((hashes as u8, open.trim_end()));
        }
        return Some((hashes as u8, rest));
    }
    None
}

fn numbered(line: &str) -> Option<(u32, &str)> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let rest = line[digits..].strip_prefix(". ")?;
    Some((line[..digits].parse().ok()?, rest))
}

/// Inline spans. A marker without its closing partner stays literal text; `\` before a
/// punctuation mark keeps the mark literal (M11ac).
fn spans(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    let mut prev: Option<char> = None;
    let closers = Closers::of(text);
    while let Some(c) = rest.chars().next() {
        let next = rest[c.len_utf8()..].chars().next();
        if let Some(n) = next.filter(|n| c == '\\' && n.is_ascii_punctuation()) {
            plain.push(n);
            prev = Some(n);
            rest = &rest[2..];
            continue;
        }
        let found = if c == '`' {
            rest[1..]
                .find('`')
                .map(|e| (Span::Code(rest[1..1 + e].to_string()), e + 2))
        } else if let Some(inner) = rest.strip_prefix("**") {
            inner
                .find("**")
                .filter(|&e| e > 0)
                .map(|e| (Span::Bold(inner[..e].to_string()), e + 4))
        } else if c == '*' || c == '_' {
            italic(rest, text.len() - rest.len(), c, prev, &closers)
        } else if c == '[' {
            link(rest)
        } else if c == '!' && next == Some('[') {
            link(&rest[1..]).map(|(span, used)| match span {
                Span::Link { text, url } => (Span::Image { alt: text, url }, used + 1),
                other => (other, used + 1),
            })
        } else if c == '<' {
            autolink(rest)
        } else {
            None
        };
        match found {
            Some((span, used)) => {
                if !plain.is_empty() {
                    out.push(Span::Text(std::mem::take(&mut plain)));
                }
                out.push(span);
                prev = rest[..used].chars().last();
                rest = &rest[used..];
            }
            None => {
                plain.push(c);
                prev = Some(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    if !plain.is_empty() {
        out.push(Span::Text(plain));
    }
    out
}

/// Where `*` and `_` can close italic text in a paragraph (review fix: found once, so finding
/// an opener's closer is a search, not a scan of the rest): byte offsets, in order, of each
/// mark with no space or same mark before it and no same mark after it (an `_` also no letter
/// or digit after it).
struct Closers {
    star: Vec<usize>,
    under: Vec<usize>,
}

impl Closers {
    fn of(text: &str) -> Closers {
        let (mut star, mut under) = (Vec::new(), Vec::new());
        let mut prev: Option<char> = None;
        let mut chars = text.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let next = chars.peek().map(|&(_, n)| n);
            if c == '*' || c == '_' {
                let closes = prev.is_some_and(|p| !p.is_whitespace() && p != c)
                    && next != Some(c)
                    && !(c == '_' && next.is_some_and(char::is_alphanumeric));
                if closes {
                    if c == '*' { &mut star } else { &mut under }.push(i);
                }
            }
            prev = Some(c);
        }
        Closers { star, under }
    }

    /// The first closer for `mark` at or after byte `from`.
    fn first(&self, mark: char, from: usize) -> Option<usize> {
        let list = if mark == '*' { &self.star } else { &self.under };
        list.get(list.partition_point(|&q| q < from)).copied()
    }
}

/// `*text*` or `_text_` at the start of `rest`, at byte `at` of the paragraph (`prev`: the
/// character before it). Its text starts and ends with no space; an `_` inside a word
/// (`snake_case`) is a letter.
fn italic(
    rest: &str,
    at: usize,
    mark: char,
    prev: Option<char>,
    closers: &Closers,
) -> Option<(Span, usize)> {
    if mark == '_' && prev.is_some_and(char::is_alphanumeric) {
        return None;
    }
    let body = &rest[1..];
    let first = body.chars().next()?;
    if first.is_whitespace() || first == mark {
        return None;
    }
    let close = closers.first(mark, at + 1 + first.len_utf8())? - at;
    Some((Span::Italic(rest[1..close].to_string()), close + 1))
}

/// `<https://…>`, `<http://…>` or `<mailto:…>`: a link showing its address. The look for its
/// `>` stops at a space or another `<` (so it's linear over a paragraph).
fn autolink(rest: &str) -> Option<(Span, usize)> {
    let end = 1 + rest[1..].find(|c: char| c == '>' || c == '<' || c.is_whitespace())?;
    if !rest[end..].starts_with('>') {
        return None;
    }
    let url = &rest[1..end];
    let known = ["https://", "http://", "mailto:"]
        .iter()
        .any(|s| url.starts_with(s));
    if !known {
        return None;
    }
    Some((
        Span::Link {
            text: url.to_string(),
            url: url.to_string(),
        },
        end + 1,
    ))
}

/// `[text](url)` at the start of `rest`. The look for `](` stops at another `[` or a line
/// break, and the one for `)` at a space or a `[` (so it's linear over a paragraph).
fn link(rest: &str) -> Option<(Span, usize)> {
    let bytes = rest.as_bytes();
    let mut close = None;
    for i in 1..bytes.len() {
        match bytes[i] {
            b'[' | b'\n' => return None,
            b']' if bytes.get(i + 1) == Some(&b'(') => {
                close = Some(i);
                break;
            }
            _ => {}
        }
    }
    let close = close?;
    let label = &rest[1..close];
    let after = &rest[close + 2..];
    let end = after.find(|c: char| c == ')' || c == '[' || c.is_whitespace())?;
    if !after[end..].starts_with(')') {
        return None;
    }
    let url = &after[..end];
    if url.is_empty() {
        return None;
    }
    Some((
        Span::Link {
            text: label.to_string(),
            url: url.to_string(),
        },
        close + 2 + end + 1,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Span {
        Span::Text(s.into())
    }

    #[test]
    fn parses_the_subset() {
        let md = parse("Hi `x` **b**\n\n- one\n1. two\n```rust\nfn f(){}\n```\n[a](https://e.x)");
        assert_eq!(
            md,
            vec![
                Md::Para(vec![
                    t("Hi "),
                    Span::Code("x".into()),
                    t(" "),
                    Span::Bold("b".into())
                ]),
                Md::Bullet(vec![t("one")]),
                Md::Numbered(1, vec![t("two")]),
                Md::Code {
                    lang: Some("rust".into()),
                    text: "fn f(){}".into()
                },
                Md::Para(vec![Span::Link {
                    text: "a".into(),
                    url: "https://e.x".into()
                }]),
            ]
        );
    }

    #[test]
    fn unterminated_fence_is_code_to_the_end() {
        assert_eq!(
            parse("```\nabc"),
            vec![Md::Code {
                lang: None,
                text: "abc".into()
            }]
        );
    }

    #[test]
    fn headings_and_unclosed_inline_markers() {
        assert_eq!(
            parse("## Title\nsome `open and **bold"),
            vec![
                Md::Heading(2, vec![t("Title")]),
                Md::Para(vec![t("some `open and **bold")]),
            ]
        );
    }

    #[test]
    fn non_ascii_text_survives() {
        assert_eq!(
            parse("héllo **wörld** é"),
            vec![Md::Para(vec![
                t("héllo "),
                Span::Bold("wörld".into()),
                t(" é")
            ])]
        );
    }
}

/// M11ac: the rest of the Markdown a README uses, for the file tab's Preview.
#[cfg(test)]
mod file_tests {
    use super::*;

    fn t(s: &str) -> Span {
        Span::Text(s.into())
    }

    #[test]
    fn italic_needs_a_closing_marker_and_spares_snake_case() {
        assert_eq!(
            parse("*a* and _b_ in snake_case_name, a * b * c, **bold**"),
            vec![Md::Para(vec![
                Span::Italic("a".into()),
                t(" and "),
                Span::Italic("b".into()),
                t(" in snake_case_name, a * b * c, "),
                Span::Bold("bold".into()),
            ])]
        );
        assert_eq!(
            parse("an *open one"),
            vec![Md::Para(vec![t("an *open one")])]
        );
    }

    #[test]
    fn escapes_images_and_autolinks() {
        assert_eq!(
            parse(r"\*not\* ![logo](a.png) <https://e.x/y>"),
            vec![Md::Para(vec![
                t("*not* "),
                Span::Image {
                    alt: "logo".into(),
                    url: "a.png".into()
                },
                t(" "),
                Span::Link {
                    text: "https://e.x/y".into(),
                    url: "https://e.x/y".into()
                },
            ])]
        );
    }

    #[test]
    fn rules_setext_headings_and_closing_hashes() {
        assert_eq!(
            parse("Title\n=====\nSub\n---\n\npara\n\n---\n***\n_ _ _\n## Closed ##"),
            vec![
                Md::Heading(1, vec![t("Title")]),
                Md::Heading(2, vec![t("Sub")]),
                Md::Para(vec![t("para")]),
                Md::Rule,
                Md::Rule,
                Md::Rule,
                Md::Heading(2, vec![t("Closed")]),
            ]
        );
    }

    #[test]
    fn block_quotes_hold_blocks() {
        assert_eq!(
            parse("> # T\n> - a\n>\n> b\nafter"),
            vec![
                Md::Quote(vec![
                    Md::Heading(1, vec![t("T")]),
                    Md::Bullet(vec![t("a")]),
                    Md::Para(vec![t("b")]),
                ]),
                Md::Para(vec![t("after")]),
            ]
        );
    }

    #[test]
    fn task_items_and_nesting_by_indent() {
        assert_eq!(
            parse(
                "- [ ] todo\n* [X] done\n  - inner\n    1. deep\n      more\n- back\n\ntext\n  - fresh"
            ),
            vec![
                Md::Task(false, vec![t("todo")]),
                Md::Task(true, vec![t("done")]),
                Md::Nested(1, Box::new(Md::Bullet(vec![t("inner")]))),
                Md::Nested(2, Box::new(Md::Numbered(1, vec![t("deep\nmore")]))),
                Md::Bullet(vec![t("back")]),
                Md::Para(vec![t("text")]),
                Md::Bullet(vec![t("fresh")]),
            ]
        );
    }

    #[test]
    fn tables_with_alignment_and_short_rows() {
        let cell = |s: &str| vec![t(s)];
        assert_eq!(
            parse(
                "| A | B | C |\n|:--|:-:|--:|\n| `x` | **y** | z |\n| 1 |\n\na | b\n--- | ---\n1 | 2"
            ),
            vec![
                Md::Table {
                    align: vec![Align::Left, Align::Center, Align::Right],
                    head: vec![cell("A"), cell("B"), cell("C")],
                    rows: vec![
                        vec![
                            vec![Span::Code("x".into())],
                            vec![Span::Bold("y".into())],
                            cell("z")
                        ],
                        vec![cell("1"), vec![], vec![]],
                    ],
                },
                Md::Table {
                    align: vec![Align::Default, Align::Default],
                    head: vec![cell("a"), cell("b")],
                    rows: vec![vec![cell("1"), cell("2")]],
                },
            ]
        );
    }

    #[test]
    fn a_table_header_still_streaming_is_a_paragraph() {
        assert_eq!(parse("| a | b |"), vec![Md::Para(vec![t("| a | b |")])]);
    }

    /// Review fix: the inline scanners are linear. 1 MB of unclosed openers (`*a `, `_a `,
    /// `[a`, `<a`, `[a](`, `![a](`) each parse within the budget: 100 ms in a release build,
    /// 3 s in a debug one (unoptimised code is 10-30x slower; a quadratic scan of 1 MB takes
    /// minutes either way, so the budget still tells them apart).
    #[test]
    fn a_megabyte_of_unclosed_markers_parses_quickly() {
        let budget = if cfg!(debug_assertions) {
            std::time::Duration::from_secs(3)
        } else {
            std::time::Duration::from_millis(100)
        };
        for unit in ["*a ", "_a ", "[a", "<a", "[a](", "![a](", "`a"] {
            let text = unit.repeat(1_000_000 / unit.len());
            let start = std::time::Instant::now();
            let blocks = parse(&text);
            let took = start.elapsed();
            assert_eq!(blocks.len(), 1, "{unit:?}: one paragraph");
            assert!(took < budget, "{unit:?}: {took:?}");
        }
    }

    /// How deep the quotes in `blocks` go.
    fn quote_depth(blocks: &[Md]) -> usize {
        blocks
            .iter()
            .map(|b| match b {
                Md::Quote(inner) => 1 + quote_depth(inner),
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }

    /// Review fix: quotes nest at most `QUOTE_DEPTH` deep; a deeper `>` stays text, so a
    /// crafted line of 50,000 `>` neither recurses past the cap nor overflows the stack.
    #[test]
    fn quotes_nest_at_most_sixteen_deep() {
        let line = ">".repeat(50_000);
        // On a small stack, as a release build's main thread has more room than this.
        let blocks = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || parse(&line))
            .unwrap()
            .join()
            .expect("no stack overflow");
        assert_eq!(quote_depth(&blocks), QUOTE_DEPTH);
        let mut inner = &blocks;
        for _ in 0..QUOTE_DEPTH {
            match &inner[..] {
                [Md::Quote(next)] => inner = next,
                other => panic!("one quote a level: {other:?}"),
            }
        }
        assert_eq!(
            inner[..],
            [Md::Para(vec![t(&">".repeat(50_000 - QUOTE_DEPTH))])]
        );
        // Two levels as before.
        assert_eq!(
            parse("> > a"),
            vec![Md::Quote(vec![Md::Quote(vec![Md::Para(vec![t("a")])])])]
        );
    }
}
