#![allow(dead_code)] // consumers arrive in M11aa Tasks 7–11; remove when wired
//! The small markdown subset agent messages use (M11aa): paragraphs, headings, bullets, numbered
//! items, fenced code, and inline code / bold / links. Pure and GPUI-free. Anything it does not
//! understand stays literal text, and an unterminated fence is code to the end (streaming).

#[derive(Clone, Debug, PartialEq)]
pub enum Md {
    Para(Vec<Span>),
    Heading(u8, Vec<Span>),
    Bullet(Vec<Span>),
    Numbered(u32, Vec<Span>),
    Code { lang: Option<String>, text: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Span {
    Text(String),
    Code(String),
    Bold(String),
    Link { text: String, url: String },
}

/// Parse message text into blocks.
pub fn parse(text: &str) -> Vec<Md> {
    let mut out = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("```") {
            flush(&mut para, &mut out);
            let lang = rest.trim();
            let mut body: Vec<&str> = Vec::new();
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                body.push(l);
            }
            out.push(Md::Code {
                lang: (!lang.is_empty()).then(|| lang.to_string()),
                text: body.join("\n"),
            });
        } else if trimmed.trim().is_empty() {
            flush(&mut para, &mut out);
        } else if let Some((level, rest)) = heading(trimmed) {
            flush(&mut para, &mut out);
            out.push(Md::Heading(level, spans(rest)));
        } else if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            flush(&mut para, &mut out);
            out.push(Md::Bullet(spans(rest.trim())));
        } else if let Some((n, rest)) = numbered(trimmed) {
            flush(&mut para, &mut out);
            out.push(Md::Numbered(n, spans(rest.trim())));
        } else {
            para.push(line.trim());
        }
    }
    flush(&mut para, &mut out);
    out
}

fn flush(para: &mut Vec<&str>, out: &mut Vec<Md>) {
    if !para.is_empty() {
        out.push(Md::Para(spans(&para.join("\n"))));
        para.clear();
    }
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) {
        let rest = line[hashes..].strip_prefix(' ')?;
        return Some((hashes as u8, rest.trim()));
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

/// Inline spans. A marker without its closing partner stays literal text.
fn spans(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let found = if c == '`' {
            rest[1..]
                .find('`')
                .map(|e| (Span::Code(rest[1..1 + e].to_string()), e + 2))
        } else if let Some(inner) = rest.strip_prefix("**") {
            inner
                .find("**")
                .filter(|&e| e > 0)
                .map(|e| (Span::Bold(inner[..e].to_string()), e + 4))
        } else if c == '[' {
            link(rest)
        } else {
            None
        };
        match found {
            Some((span, used)) => {
                if !plain.is_empty() {
                    out.push(Span::Text(std::mem::take(&mut plain)));
                }
                out.push(span);
                rest = &rest[used..];
            }
            None => {
                plain.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    if !plain.is_empty() {
        out.push(Span::Text(plain));
    }
    out
}

fn link(rest: &str) -> Option<(Span, usize)> {
    let close = rest.find("](")?;
    let label = &rest[1..close];
    if label.contains('[') || label.contains('\n') {
        return None;
    }
    let after = &rest[close + 2..];
    let end = after.find(')')?;
    let url = &after[..end];
    if url.is_empty() || url.contains(char::is_whitespace) {
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
