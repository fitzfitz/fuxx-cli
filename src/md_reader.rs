//! M11ai (the Markdown reader): the pure half of the Preview's new looks. Front matter is split
//! off before the parse; a `ReaderPlan` is worked out beside the parse, once per version: which
//! top-level quotes are callouts, which paragraphs are badge rows, each heading's slug, and the
//! "On this page" outline. Nothing here draws or touches GPUI, so all of it is tested.
#![allow(dead_code)] // the Preview draws from it as M11ai's tasks land (Task 11 removes any left)

use crate::agent_markdown::{self, Md, Span};
use crate::ui_tokens::reader::{FRONT_ROWS, JUMP_ROOM};
use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;

use alacritty_terminal::vte::ansi::Rgb;

use crate::file_tab::{TAB_WIDTH, expand_tabs};
use crate::highlight::Span as HlSpan;

#[derive(Clone, Debug, PartialEq)]
pub struct FrontTable {
    pub rows: Vec<(String, String)>,
    pub more: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Front {
    Table(FrontTable),
    Raw(String),
}

/// The front matter and the text after its closing line (`None`: the whole text).
pub fn front_matter(text: &str) -> (Option<Front>, &str) {
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = body.split_inclusive('\n');
    match lines.next() {
        Some(first) if first.trim_end() == "---" => {}
        _ => return (None, text),
    }
    let start = body.len() - lines.clone().map(str::len).sum::<usize>();
    let mut at = start;
    for line in lines {
        let t = line.trim_end();
        if t == "---" || t == "..." {
            let block = &body[start..at];
            return (Some(parse_front(block)), &body[at + line.len()..]);
        }
        at += line.len();
    }
    (None, text)
}

/// Drop one matching pair of quotes round a value.
fn unquote(v: &str) -> &str {
    let v = v.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return v[1..v.len() - 1].trim();
        }
    }
    v
}

/// A line's key, if it is a top-level `key: value` (colon then space or end): (key, value).
fn key_line(line: &str) -> Option<(&str, &str)> {
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    let mut from = 0;
    while let Some(i) = line[from..].find(':') {
        let at = from + i;
        let after = &line[at + 1..];
        if after.is_empty() || after.starts_with(' ') {
            let key = line[..at].trim_end();
            return (!key.is_empty()).then_some((key, after.trim()));
        }
        from = at + 1;
    }
    None
}

fn parse_front(block: &str) -> Front {
    let mut pairs: Vec<(String, Vec<String>, bool)> = Vec::new(); // key, parts, text block
    for line in block.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = key_line(line) {
            let text_block = value.starts_with('|') || value.starts_with('>');
            let mut parts = Vec::new();
            if !value.is_empty() && !text_block {
                let v = value
                    .strip_prefix('[')
                    .and_then(|v| v.strip_suffix(']'))
                    .unwrap_or(value);
                parts.push(unquote(v).to_string());
            }
            pairs.push((key.to_string(), parts, text_block));
        } else if line.starts_with(char::is_whitespace) || line.starts_with("- ") || t == "-" {
            match pairs.last_mut() {
                Some((_, parts, _)) => {
                    let item = t.strip_prefix("- ").unwrap_or(t.trim_start_matches('-'));
                    parts.push(unquote(item).to_string());
                }
                None => return Front::Raw(block.trim().to_string()),
            }
        } else {
            return Front::Raw(block.trim().to_string());
        }
    }
    let mut rows: Vec<(String, String)> = pairs
        .into_iter()
        .map(|(k, parts, text_block)| {
            let sep = if text_block { " " } else { ", " };
            (k, parts.join(sep))
        })
        .collect();
    let more = rows.len().saturating_sub(FRONT_ROWS);
    rows.truncate(FRONT_ROWS);
    Front::Table(FrontTable { rows, more })
}

/// A heading's plain text: text, code, bold, italic and link text; no image alt.
pub fn heading_text(spans: &[Span]) -> String {
    let mut out = String::new();
    for s in spans {
        match s {
            Span::Text(t) | Span::Code(t) | Span::Bold(t) | Span::Italic(t) => out.push_str(t),
            Span::Link { text, .. } => out.push_str(text),
            Span::Image { .. } | Span::LinkedImage { .. } => {}
        }
    }
    out
}

/// GitHub's heading slug.
pub fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// Slugs for headings in order; a repeat gets `-1`, `-2`, counted per base slug.
pub fn slugs(texts: &[String]) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    texts
        .iter()
        .map(|t| {
            let base = slug(t);
            let n = seen.entry(base.clone()).or_insert(0);
            let out = if *n == 0 { base } else { format!("{base}-{n}") };
            *n += 1;
            out
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callout {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl Callout {
    pub const ALL: [Callout; 5] = [
        Callout::Note,
        Callout::Tip,
        Callout::Important,
        Callout::Warning,
        Callout::Caution,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Callout::Note => "Note",
            Callout::Tip => "Tip",
            Callout::Important => "Important",
            Callout::Warning => "Warning",
            Callout::Caution => "Caution",
        }
    }

    fn marker(line: &str) -> Option<Callout> {
        let inner = line.trim().strip_prefix("[!")?.strip_suffix(']')?;
        Callout::ALL
            .into_iter()
            .find(|c| c.title().eq_ignore_ascii_case(inner))
    }
}

/// A top-level quote's blocks as a callout: its kind and its body (the marker line taken out).
pub fn callout(inner: &[Md]) -> Option<(Callout, Vec<Md>)> {
    let Some(Md::Para(spans)) = inner.first() else {
        return None;
    };
    let Some(Span::Text(first)) = spans.first() else {
        return None;
    };
    let (line, rest) = match first.split_once('\n') {
        Some((line, rest)) => (line, Some(rest)),
        None => (first.as_str(), None),
    };
    let kind = Callout::marker(line)?;
    if rest.is_none() && spans.len() > 1 {
        return None; // the marker is not alone on its line
    }
    let mut para: Vec<Span> = Vec::new();
    if let Some(r) = rest.filter(|r| !r.is_empty()) {
        para.push(Span::Text(r.to_string()));
    }
    para.extend(spans[1..].iter().cloned());
    let blank = para
        .iter()
        .all(|s| matches!(s, Span::Text(t) if t.trim().is_empty()));
    let mut body = Vec::new();
    if !blank {
        body.push(Md::Para(para));
    }
    body.extend(inner[1..].iter().cloned());
    Some((kind, body))
}

/// Whether an image source looks like a badge's.
pub fn is_badge_src(src: &str) -> bool {
    let (host, path) = match src.split_once("://") {
        Some((_, rest)) => match rest.find('/') {
            Some(i) => (rest[..i].to_ascii_lowercase(), &rest[i..]),
            None => (rest.to_ascii_lowercase(), ""),
        },
        None => (String::new(), src),
    };
    let host = host.rsplit('@').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    const HOSTS: [&str; 4] = [
        "img.shields.io",
        "badgen.net",
        "badge.fury.io",
        "codecov.io",
    ];
    if HOSTS
        .iter()
        .any(|h| host == *h || host.ends_with(&format!(".{h}")))
    {
        return true;
    }
    let path = path.split(['?', '#']).next().unwrap_or("");
    path.to_ascii_lowercase().ends_with("badge.svg")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Badge {
    pub label: String,
    pub value: Option<String>,
    pub url: Option<String>,
}

/// `"license: MIT"` becomes ("license", Some("MIT")); `""` becomes ("badge", None) (drawn dim).
pub fn pill(alt: &str) -> (String, Option<String>) {
    let alt = alt.trim();
    if alt.is_empty() {
        return ("badge".to_string(), None);
    }
    match alt.split_once(": ") {
        Some((label, value)) if !label.trim().is_empty() && !value.trim().is_empty() => {
            (label.trim().to_string(), Some(value.trim().to_string()))
        }
        _ => (alt.to_string(), None),
    }
}

/// A paragraph made only of badges and spaces: its badges.
pub fn badge_row(spans: &[Span]) -> Option<Vec<Badge>> {
    let mut badges = Vec::new();
    let mut all_badge_src = true;
    for s in spans {
        let (alt, src, url) = match s {
            Span::Text(t) if t.trim().is_empty() => continue,
            Span::Image { alt, url } => (alt, url, None),
            Span::LinkedImage { alt, src, url } => (alt, src, Some(url.clone())),
            _ => return None,
        };
        all_badge_src &= is_badge_src(src);
        let (label, value) = pill(alt);
        badges.push(Badge { label, value, url });
    }
    (badges.len() >= 2 || (badges.len() == 1 && all_badge_src)).then_some(badges)
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlockLook {
    Plain,
    Callout(Callout, Vec<Md>),
    Badges(Vec<Badge>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Heading {
    pub block: usize,
    pub level: u8,
    pub text: String,
    pub slug: String,
}

/// An "On this page" entry: an index into `headings`.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub heading: usize,
    pub indent: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Front,
    Block(usize),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReaderPlan {
    pub front: Option<Front>,
    pub looks: Vec<BlockLook>,
    pub headings: Vec<Heading>,
    pub outline: Vec<Entry>,
}

impl ReaderPlan {
    pub fn of(front: Option<Front>, blocks: &[Md]) -> ReaderPlan {
        let looks = blocks
            .iter()
            .map(|b| match b {
                Md::Quote(inner) => match callout(inner) {
                    Some((kind, body)) => BlockLook::Callout(kind, body),
                    None => BlockLook::Plain,
                },
                Md::Para(spans) => match badge_row(spans) {
                    Some(badges) => BlockLook::Badges(badges),
                    None => BlockLook::Plain,
                },
                _ => BlockLook::Plain,
            })
            .collect();
        let found: Vec<(usize, u8, String)> = blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match b {
                Md::Heading(level, spans) => Some((i, *level, heading_text(spans))),
                _ => None,
            })
            .collect();
        let texts: Vec<String> = found.iter().map(|(_, _, t)| t.clone()).collect();
        let headings: Vec<Heading> = found
            .into_iter()
            .zip(slugs(&texts))
            .map(|((block, level, text), slug)| Heading {
                block,
                level,
                text,
                slug,
            })
            .collect();
        let h1s = headings.iter().filter(|h| h.level == 1).count();
        let title_first = h1s == 1 && headings.first().is_some_and(|h| h.level == 1);
        let mut outline: Vec<Entry> = headings
            .iter()
            .enumerate()
            .filter(|(i, h)| h.level <= 3 && !(title_first && *i == 0))
            .map(|(i, h)| Entry {
                heading: i,
                indent: h.level == 3,
            })
            .collect();
        if outline.len() < 2 {
            outline.clear();
        }
        ReaderPlan {
            front,
            looks,
            headings,
            outline,
        }
    }

    /// List rows: the blocks, plus one for the front matter when there is some to draw.
    pub fn rows(&self) -> usize {
        self.looks.len() + self.front_rows()
    }

    pub fn row_kind(&self, row: usize) -> RowKind {
        if self.front_rows() == 1 && row == 0 {
            RowKind::Front
        } else {
            RowKind::Block(row - self.front_rows())
        }
    }

    pub fn row_of_block(&self, block: usize) -> usize {
        block + self.front_rows()
    }

    /// 1 when a front matter table (or raw block) is drawn, else 0. An empty block draws nothing.
    pub fn front_rows(&self) -> usize {
        match &self.front {
            None => 0,
            Some(Front::Table(t)) if t.rows.is_empty() && t.more == 0 => 0,
            Some(_) => 1,
        }
    }

    /// The row of the heading a `#part` names: the slug exactly, else its lower case; the part
    /// is percent-decoded (plan-time call 11).
    pub fn anchor_row(&self, anchor: &str) -> Option<usize> {
        let part = percent_decode(anchor);
        let lower = part.to_lowercase();
        let at = self
            .headings
            .iter()
            .find(|h| h.slug == part)
            .or_else(|| self.headings.iter().find(|h| h.slug == lower))?;
        Some(self.row_of_block(at.block))
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Split, parse and plan a file's text (once per version).
pub fn read(text: &str) -> (Vec<Md>, ReaderPlan) {
    let (front, body) = front_matter(text);
    let blocks = agent_markdown::parse(body);
    let plan = ReaderPlan::of(front, &blocks);
    (blocks, plan)
}

/// Which outline entry is current (plan-time call 12): `rows[i]` is entry i's list row.
pub fn current_entry(
    rows: &[usize],
    at_end: bool,
    reached: impl Fn(usize) -> bool,
    in_view: impl Fn(usize) -> bool,
) -> Option<usize> {
    if at_end && let Some(i) = rows.iter().rposition(|r| in_view(*r)) {
        return Some(i);
    }
    rows.iter().rposition(|r| reached(*r))
}

/// How far into a heading's row a jump scrolls, so its text is `JUMP_ROOM` under the top.
pub fn jump_offset(gap_above: f32) -> f32 {
    (gap_above - JUMP_ROOM).max(0.0)
}

/// The copied heading link (plan-time call 10). Outside a repo and outside home: the full path.
pub fn heading_link(rel: Option<&Path>, path: &Path, home: Option<&Path>, slug: &str) -> String {
    let shown = match (rel, home.and_then(|h| path.strip_prefix(h).ok())) {
        (Some(rel), _) => rel.display().to_string(),
        (None, Some(under)) => format!("~/{}", under.display()),
        (None, None) => path.display().to_string(),
    };
    format!("{shown}#{slug}")
}

/// The fence's language (plan-time call 8): its first word.
pub fn fence_lang(lang: Option<&str>) -> Option<&str> {
    let l = lang?.trim().trim_start_matches('{').trim_start_matches('.');
    let end = l
        .find(|c: char| c.is_whitespace() || matches!(c, ',' | '{' | '}'))
        .unwrap_or(l.len());
    let word = &l[..end];
    (!word.is_empty()).then_some(word)
}

/// A code block as drawn (M11ai): its text with tabs widened (`file_tab::expand_tabs`, lines
/// joined with `\n` as `str::lines` splits them), and its colour runs (`lines`: the
/// highlighter's spans, one `Vec` per line of `text`) moved onto that text, each colour passed
/// through `fit`.
pub fn code_runs(
    text: &str,
    lines: &[Vec<HlSpan>],
    fit: impl Fn(Rgb) -> Rgb,
) -> (String, Vec<(Range<usize>, Rgb)>) {
    let (mut shown, mut runs) = (String::new(), Vec::new());
    for (i, line) in text.split_inclusive('\n').enumerate() {
        if i > 0 {
            shown.push('\n');
        }
        let body = line.strip_suffix('\n').unwrap_or(line);
        let body = body.strip_suffix('\r').unwrap_or(body);
        let tabs: Vec<usize> = body.match_indices('\t').map(|(at, _)| at).collect();
        // Byte `b` of the line, in `shown`: each tab before it is `TAB_WIDTH` bytes there.
        let base = shown.len();
        let at = |b: usize| {
            let b = b.min(body.len());
            base + b + tabs.partition_point(|&t| t < b) * (TAB_WIDTH - 1)
        };
        for (r, c) in lines.get(i).into_iter().flatten() {
            let (from, to) = (at(r.start), at(r.end));
            if from < to {
                runs.push((from..to, fit(*c)));
            }
        }
        shown.push_str(&expand_tabs(body));
    }
    (shown, runs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn t(s: &str) -> Span {
        Span::Text(s.to_string())
    }
    fn img(alt: &str, url: &str) -> Span {
        Span::Image {
            alt: alt.to_string(),
            url: url.to_string(),
        }
    }
    fn linked(alt: &str, src: &str, url: &str) -> Span {
        Span::LinkedImage {
            alt: alt.to_string(),
            src: src.to_string(),
            url: url.to_string(),
        }
    }
    fn pair(k: &str, v: &str) -> (String, String) {
        (k.to_string(), v.to_string())
    }
    fn table(rows: Vec<(String, String)>, more: usize) -> Option<Front> {
        Some(Front::Table(FrontTable { rows, more }))
    }
    fn quote(text: &str) -> Vec<Md> {
        match agent_markdown::parse(text).remove(0) {
            Md::Quote(inner) => inner,
            other => panic!("not a quote: {other:?}"),
        }
    }

    #[test]
    fn front_matter_reads_key_value_pairs() {
        let text = "---\ntitle: Setup guide\nstatus: draft\ntags: [docs, setup]\n---\n# Hi";
        let (front, body) = front_matter(text);
        assert_eq!(
            front,
            table(
                vec![
                    pair("title", "Setup guide"),
                    pair("status", "draft"),
                    pair("tags", "docs, setup")
                ],
                0
            )
        );
        assert_eq!(body, "# Hi");

        let (front, _) = front_matter("---\ntags:\n  - item\n  - item\n---\nA");
        assert_eq!(front, table(vec![pair("tags", "item, item")], 0));
        let (front, _) = front_matter("---\ntags:\n- a\n- b\n---\nA");
        assert_eq!(front, table(vec![pair("tags", "a, b")], 0));
        let (front, _) = front_matter("---\ntitle: \"Quoted\"\n---\nA");
        assert_eq!(front, table(vec![pair("title", "Quoted")], 0));
        let (front, body) = front_matter("---\ntitle: x\n...\nA");
        assert_eq!(front, table(vec![pair("title", "x")], 0));
        assert_eq!(body, "A");
        let (front, _) = front_matter("---\nnote: |\n  one\n  two\n---\nA");
        assert_eq!(front, table(vec![pair("note", "one two")], 0));

        let many: String = (0..25).map(|i| format!("k{i}: v\n")).collect();
        let (front, _) = front_matter(&format!("---\n{many}---\nA"));
        let Some(Front::Table(tbl)) = front else {
            panic!("not a table")
        };
        assert_eq!((tbl.rows.len(), tbl.more), (20, 5));
    }

    #[test]
    fn front_matter_needs_its_closing_line() {
        let whole = "---\ntitle: x\n\nSome text";
        assert_eq!(front_matter(whole), (None, whole));
        assert_eq!(front_matter("---\n---\nA"), (table(vec![], 0), "A"));
        let (front, body) = front_matter("\u{feff}---\ntitle: x\n---\nA");
        assert_eq!(front, table(vec![pair("title", "x")], 0));
        assert_eq!(body, "A");
        let (front, _) = front_matter("---  \ntitle: x\n---\nA");
        assert!(front.is_some());
    }

    #[test]
    fn a_rule_then_text_is_not_front_matter() {
        let whole = "Intro\n\n---\n\nMore";
        assert_eq!(front_matter(whole), (None, whole));
    }

    #[test]
    fn front_matter_that_is_not_pairs_is_raw() {
        let (front, body) = front_matter("---\njust some words\n---\n");
        assert_eq!(front, Some(Front::Raw("just some words".to_string())));
        assert_eq!(body, "");
    }

    #[test]
    fn slugs_follow_github() {
        assert_eq!(slug("Getting Started!"), "getting-started");
        assert_eq!(slug("Café au lait"), "café-au-lait");
        assert_eq!(slug("🚀 Launch"), "-launch");
        assert_eq!(slug("API_v2 - notes"), "api_v2---notes");
        let spans = vec![
            Span::Code("fn".to_string()),
            t(" & "),
            Span::Link {
                text: "docs".to_string(),
                url: "u".to_string(),
            },
            img("skip", "a.png"),
        ];
        assert_eq!(heading_text(&spans), "fn & docs");
        assert_eq!(slug(&heading_text(&spans)), "fn--docs");
    }

    #[test]
    fn repeated_and_empty_slugs_stay_unique() {
        let texts: Vec<String> = ["Intro", "Intro", "!!!", "!!!", "Intro"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(slugs(&texts), ["intro", "intro-1", "", "-1", "intro-2"]);
    }

    #[test]
    fn callouts_by_marker() {
        for (marker, kind) in [
            ("[!NOTE]", Callout::Note),
            ("[!tip]", Callout::Tip),
            ("[!IMPORTANT]", Callout::Important),
            ("[!WARNING]", Callout::Warning),
            ("[!CAUTION]", Callout::Caution),
        ] {
            let inner = quote(&format!("> {marker}\n> Body text."));
            let (got, body) = callout(&inner).expect(marker);
            assert_eq!(got, kind);
            assert_eq!(body, vec![Md::Para(vec![t("Body text.")])]);
        }
        let inner = quote(">   [!Note]  \n> Body");
        assert_eq!(callout(&inner).map(|c| c.0), Some(Callout::Note));
        assert_eq!(callout(&quote("> [!FOO]\n> x")), None);
        assert_eq!(callout(&quote("> [!NOTE] text")), None);
        assert_eq!(callout(&quote("> plain quote")), None);
        let (kind, body) = callout(&quote("> [!TIP]")).unwrap();
        assert_eq!((kind, body), (Callout::Tip, vec![]));
        // later blocks of the quote follow the first paragraph's rest
        let (_, body) = callout(&quote("> [!NOTE]\n> a\n>\n> b")).unwrap();
        assert_eq!(body.len(), 2);
        assert_eq!(Callout::Caution.title(), "Caution");
        // a marker inside a quote stays plain: only a top-level quote can be a callout
        let blocks = agent_markdown::parse("> > [!NOTE]\n> > x");
        let plan = ReaderPlan::of(None, &blocks);
        assert!(plan.looks.iter().all(|l| *l == BlockLook::Plain));
    }

    #[test]
    fn badges_by_source_or_by_company() {
        for src in [
            "https://img.shields.io/badge/a-b-green",
            "https://badgen.net/npm/v/x",
            "https://badge.fury.io/js/x.svg",
            "https://codecov.io/gh/a/b/branch/main/graph/badge.png",
            "https://example.com/ci/badge.svg?branch=main",
        ] {
            assert!(is_badge_src(src), "{src}");
        }
        assert!(!is_badge_src("logo.png"));
        assert!(!is_badge_src("https://example.com/img.shields.io.png"));

        let shields = "https://img.shields.io/badge/license-MIT-blue";
        let row = badge_row(&[
            linked("build: passing", shields, "https://example.com/ci"),
            t(" "),
            img("license: MIT", shields),
        ])
        .unwrap();
        assert_eq!(row.len(), 2);
        assert_eq!(row[0].url.as_deref(), Some("https://example.com/ci"));
        assert_eq!(row[0].value.as_deref(), Some("passing"));
        assert_eq!(row[1].url, None);
        assert_eq!(
            badge_row(&[img("a", "a.png"), t(" "), img("b", "b.png")])
                .unwrap()
                .len(),
            2
        );
        assert_eq!(badge_row(&[img("a", "a.png")]), None);
        assert!(badge_row(&[img("a", shields)]).is_some());
        assert_eq!(
            badge_row(&[img("a", "a.png"), t(" and words"), img("b", "b.png")]),
            None
        );
        assert_eq!(badge_row(&[t(" ")]), None);
    }

    #[test]
    fn pills_split_at_colon_space() {
        assert_eq!(
            pill("license: MIT"),
            ("license".to_string(), Some("MIT".to_string()))
        );
        assert_eq!(pill("build"), ("build".to_string(), None));
        assert_eq!(pill("a:b"), ("a:b".to_string(), None));
        assert_eq!(pill(""), ("badge".to_string(), None));
    }

    #[test]
    fn the_plan_maps_rows_headings_and_outline() {
        let text = "---\ntitle: Doc\n---\n# Title\n\n## A\n\n### B\n\n## A\n\n> [!NOTE]\n> hi\n\n\
                    ![a](x.png) ![b](y.png)\n";
        let (blocks, plan) = read(text);
        assert_eq!(blocks.len(), 6);
        assert_eq!(plan.rows(), 7);
        assert_eq!(plan.front_rows(), 1);
        assert_eq!(plan.row_kind(0), RowKind::Front);
        assert_eq!(plan.row_kind(1), RowKind::Block(0));
        assert_eq!(plan.row_of_block(0), 1);
        let slugs: Vec<&str> = plan.headings.iter().map(|h| h.slug.as_str()).collect();
        assert_eq!(slugs, ["title", "a", "b", "a-1"]);
        assert_eq!(plan.outline.len(), 3);
        assert_eq!(plan.outline[0].heading, 1);
        assert!(plan.outline[1].indent && !plan.outline[0].indent);
        assert_eq!(plan.anchor_row("A-1"), Some(plan.row_of_block(3)));
        assert_eq!(plan.anchor_row("a-1"), Some(plan.row_of_block(3)));
        assert_eq!(plan.anchor_row("nope"), None);
        assert!(matches!(
            plan.looks[4],
            BlockLook::Callout(Callout::Note, _)
        ));
        assert!(matches!(plan.looks[5], BlockLook::Badges(_)));
        assert_eq!(plan.looks.len(), 6);

        let (_, one) = read("## Only\n\ntext");
        assert!(one.outline.is_empty());
        assert_eq!(one.rows(), 2);
        // an empty front matter block draws nothing
        let (_, empty) = read("---\n---\nA");
        assert_eq!(empty.rows(), 1);
        // percent-decoded anchors
        let (_, caf) = read("# Top\n\n## Café\n\n## Two\n");
        assert_eq!(caf.anchor_row("caf%C3%A9"), Some(1));
    }

    #[test]
    fn the_current_entry_is_the_last_one_reached() {
        let rows = [2, 10, 30];
        assert_eq!(current_entry(&rows, false, |r| r <= 12, |_| false), Some(1));
        assert_eq!(current_entry(&rows, false, |_| false, |_| true), None);
        assert_eq!(
            current_entry(&rows, true, |r| r <= 12, |r| r >= 25),
            Some(2)
        );
        assert_eq!(current_entry(&[], false, |_| true, |_| true), None);
    }

    #[test]
    fn a_jump_leaves_room_above_the_heading() {
        assert_eq!(jump_offset(32.0), 16.0);
        assert_eq!(jump_offset(12.0), 0.0);
    }

    #[test]
    fn heading_links_are_repo_relative() {
        let path = PathBuf::from("/h/code/my-app/docs/guide.md");
        assert_eq!(
            heading_link(Some(Path::new("docs/guide.md")), &path, None, "install"),
            "docs/guide.md#install"
        );
        let notes = PathBuf::from("/h/notes/a.md");
        assert_eq!(
            heading_link(None, &notes, Some(Path::new("/h")), "install"),
            "~/notes/a.md#install"
        );
        let far = PathBuf::from("/srv/a.md");
        assert_eq!(
            heading_link(None, &far, Some(Path::new("/h")), "install"),
            "/srv/a.md#install"
        );
    }

    #[test]
    fn fence_lang_is_the_first_word() {
        assert_eq!(fence_lang(Some("rust,ignore")), Some("rust"));
        assert_eq!(fence_lang(Some("{.python}")), Some("python"));
        assert_eq!(fence_lang(Some("sh title=\"x\"")), Some("sh"));
        assert_eq!(fence_lang(Some("")), None);
        assert_eq!(fence_lang(None), None);
    }

    #[test]
    fn read_plans_a_megabyte_quickly() {
        use std::time::Instant;
        let mut text = String::from("---\ntitle: Big\n---\n");
        let mut n = 0;
        while text.len() < 1_000_000 {
            text.push_str(&format!(
                "## Section {n}\n\nSome *words* and `code` with a [link](a.md#x) in it.\n\n\
                 > [!NOTE]\n> A callout {n}.\n\n- one\n- two\n\n```rust\nfn f() {{}}\n```\n\n\
                 ![a](https://img.shields.io/b.svg) ![b](c.png)\n\n"
            ));
            n += 1;
        }
        let t0 = Instant::now();
        let (front, body) = front_matter(&text);
        let blocks = agent_markdown::parse(body);
        let parse = t0.elapsed();
        let t1 = Instant::now();
        let plan = ReaderPlan::of(front, &blocks);
        let planned = t1.elapsed();
        println!(
            "read_plans_a_megabyte_quickly: {} blocks, parse {parse:?}, plan {planned:?}",
            blocks.len()
        );
        assert_eq!(plan.rows(), blocks.len() + 1);
        assert!(planned.as_secs_f64() <= parse.as_secs_f64() * 1.5);
    }

    #[test]
    fn code_runs_widen_tabs_and_keep_colours() {
        let (red, blue) = (Rgb { r: 200, g: 0, b: 0 }, Rgb { r: 0, g: 0, b: 200 });
        let fitted = Rgb { r: 1, g: 2, b: 3 };
        // `let` (bytes 1..4 after the tab), the rest of the line, and `foo`.
        let lines = vec![
            vec![(0..1, blue), (1..4, red), (4..8, blue)],
            vec![(0..3, blue)],
        ];
        let fit = |c: Rgb| if c == red { fitted } else { c };
        let (shown, runs) = code_runs("\tlet x;\nfoo", &lines, fit);
        assert_eq!(shown, "    let x;\nfoo");
        assert_eq!(
            runs,
            vec![(0..4, blue), (4..7, fitted), (7..10, blue), (11..14, blue)]
        );
        assert_eq!(&shown[4..7], "let");
        // A trailing `\r\n` is dropped as `lines` drops it; no spans: no runs.
        let (shown, runs) = code_runs("a\r\nb", &[], |c| c);
        assert_eq!((shown.as_str(), runs), ("a\nb", vec![]));
    }
}
