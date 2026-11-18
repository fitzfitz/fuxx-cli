//! Syntax colours for read-only file tabs (M11g): file-size limits and a syntect highlighter whose
//! theme is built from the fuxx terminal theme, so file colours follow the chosen theme.

use crate::theme::Theme;
use crate::ui_tokens::{MARK_MIN, ensure_contrast};
use alacritty_terminal::vte::ansi::Rgb;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use syntect::highlighting::{
    Color, HighlightIterator, HighlightState, ScopeSelectors, StyleModifier, ThemeItem,
    ThemeSettings,
};
use syntect::parsing::{ParseState, ScopeStack, ScopeStackOp, SyntaxReference, SyntaxSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Viewable {
    Text,
    PlainTooLarge,
    Refuse,
}

/// Over this many bytes a file is shown as plain text, without colours.
pub(crate) const PLAIN_ABOVE: u64 = 2 * 1024 * 1024;
/// Over this many bytes a file is not opened at all.
pub(crate) const REFUSE_ABOVE: u64 = 20 * 1024 * 1024;

/// How a file may be shown, from its size and the first bytes (at most 8 KB): a NUL byte there
/// means binary.
pub fn classify(len: u64, head: &[u8]) -> Viewable {
    if len > REFUSE_ABOVE || head.contains(&0) {
        Viewable::Refuse
    } else if len > PLAIN_ABOVE {
        Viewable::PlainTooLarge
    } else {
        Viewable::Text
    }
}

/// The syntax definitions and the colour theme. Loading the syntaxes takes tens of milliseconds, so
/// call `load` off the UI thread.
pub struct Highlighter {
    /// Shared by every theme's highlighter (`with_theme`): loaded once.
    syntaxes: Arc<SyntaxSet>,
    theme: syntect::highlighting::Theme,
    foreground: Rgb,
}

/// The scopes that name a block's opener (M11ah): a line with one pushed is a named line.
const NAMED_SCOPES: &str = "entity.name.function, entity.name.class, entity.name.type, \
     entity.name.struct, entity.name.enum, entity.name.trait, entity.name.impl, \
     entity.name.module, entity.name.namespace, entity.name.section, markup.heading";

/// A byte range within one line, and its colour.
pub type Span = (std::ops::Range<usize>, Rgb);

/// A text coloured: each line's spans, and the lines that name something (a function, a type, a
/// heading...), ascending.
pub struct Coloured {
    pub spans: Vec<Vec<Span>>,
    pub named: Vec<usize>,
}

impl Highlighter {
    /// Blocking: loads the bundled syntaxes (the `extra_newlines` set, for line-by-line use).
    pub fn load(theme: &Theme) -> Highlighter {
        Highlighter {
            syntaxes: Arc::new(two_face::syntax::extra_newlines()),
            theme: syntect_theme(theme),
            foreground: theme.foreground,
        }
    }

    /// The same syntaxes (shared, not loaded again) in `theme`'s colours. Cheap: for a theme
    /// change while other highlights still use this one.
    pub fn with_theme(&self, theme: &Theme) -> Highlighter {
        Highlighter {
            syntaxes: self.syntaxes.clone(),
            theme: syntect_theme(theme),
            foreground: theme.foreground,
        }
    }

    /// The syntax for `path` (by extension, then by file name, then by `first_line`), if any.
    fn find_syntax(&self, path: &Path, first_line: &str) -> Option<&SyntaxReference> {
        let by_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|e| self.syntaxes.find_syntax_by_extension(e));
        // Files with no extension (`Makefile`, `Dockerfile`) are registered by name.
        let by_name = || {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| self.syntaxes.find_syntax_by_extension(n))
        };
        by_ext
            .or_else(by_name)
            .or_else(|| self.syntaxes.find_syntax_by_first_line(first_line))
    }

    /// The name of the syntax `colour` would use for this file (the same lookup), or
    /// `"Plain Text"` when there is none.
    pub fn syntax_name(&self, path: &Path, first_line: &str) -> String {
        self.find_syntax(path, first_line)
            .map_or_else(|| "Plain Text".to_string(), |s| s.name.clone())
    }

    /// M11ai: `text` (a fenced code block's) coloured as the syntax `lang` names (its
    /// extension or its name, any case: `find_syntax_by_token`), one `Vec` of spans per line;
    /// `None` when no syntax has that name.
    pub fn highlight_lang(&self, lang: &str, text: &str) -> Option<Vec<Vec<Span>>> {
        let syntax = self.syntaxes.find_syntax_by_token(lang)?;
        Some(self.colour_with(syntax, text).spans)
    }

    /// One `Vec` of spans per line (a line keeps its `\n`, so the ranges cover every byte), and
    /// the lines that name something: after each scope pushed, the stack is matched against
    /// `NAMED_SCOPES`. The syntax is found by file extension, then by the first line; otherwise
    /// the text is plain (nothing named). An empty text is one empty line.
    pub fn colour(&self, path: &Path, text: &str) -> Coloured {
        if text.is_empty() {
            return Coloured {
                spans: vec![Vec::new()],
                named: Vec::new(),
            };
        }
        let first_line = text.split_inclusive('\n').next().unwrap_or("");
        let Some(syntax) = self.find_syntax(path, first_line) else {
            return Coloured {
                spans: text
                    .split_inclusive('\n')
                    .map(|l| vec![(0..l.len(), self.foreground)])
                    .collect(),
                named: Vec::new(),
            };
        };
        self.colour_with(syntax, text)
    }

    /// `text` coloured line by line as `syntax` (`colour`'s and `highlight_lang`'s loop).
    fn colour_with(&self, syntax: &SyntaxReference, text: &str) -> Coloured {
        let highlighter = syntect::highlighting::Highlighter::new(&self.theme);
        let mut colours = HighlightState::new(&highlighter, ScopeStack::new());
        let mut parse = ParseState::new(syntax);
        // The scopes open, kept here as well: the colours' own copy is private.
        let mut stack = ScopeStack::new();
        let names = ScopeSelectors::from_str(NAMED_SCOPES).expect("a valid scope selector");
        let (mut spans_by_line, mut named) = (Vec::new(), Vec::new());
        for (i, line) in text.split_inclusive('\n').enumerate() {
            let Ok(ops) = parse.parse_line(line, &self.syntaxes) else {
                // A parse error: show the line plain.
                spans_by_line.push(vec![(0..line.len(), self.foreground)]);
                continue;
            };
            let mut names_something = false;
            for (_, op) in &ops {
                if stack.apply(op).is_ok()
                    && matches!(op, ScopeStackOp::Push(_))
                    && !names_something
                {
                    names_something = names.does_match(stack.as_slice()).is_some();
                }
            }
            if names_something {
                named.push(i);
            }
            let mut spans: Vec<Span> = Vec::new();
            let mut at = 0;
            for (style, piece) in HighlightIterator::new(&mut colours, &ops, line, &highlighter) {
                let c = style.foreground;
                let rgb = Rgb {
                    r: c.r,
                    g: c.g,
                    b: c.b,
                };
                let end = at + piece.len();
                match spans.last_mut() {
                    Some((r, prev)) if *prev == rgb => r.end = end,
                    _ => spans.push((at..end, rgb)),
                }
                at = end;
            }
            spans_by_line.push(spans);
        }
        Coloured {
            spans: spans_by_line,
            named,
        }
    }
}

fn color(c: Rgb) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a: 0xff,
    }
}

/// A syntect theme whose colours are the fuxx theme's own: comments bright-black, strings green,
/// keywords and storage magenta, constants and numbers yellow, functions blue, types cyan, invalid
/// red, everything else the foreground.
pub fn syntect_theme(theme: &Theme) -> syntect::highlighting::Theme {
    let p = &theme.palette;
    let rules: [(&str, Rgb); 9] = [
        ("comment", p[8]),
        ("string", p[2]),
        ("keyword, storage", p[5]),
        ("constant, constant.numeric", p[3]),
        ("entity.name.function, support.function", p[4]),
        (
            "entity.name.type, entity.name.class, support.type, support.class",
            p[6],
        ),
        ("entity.name.tag, markup.heading", p[4]),
        ("invalid", p[1]),
        ("variable.parameter", theme.foreground),
    ];
    syntect::highlighting::Theme {
        name: Some("fuxx".to_string()),
        author: None,
        settings: ThemeSettings {
            foreground: Some(color(theme.foreground)),
            background: Some(color(theme.background)),
            ..ThemeSettings::default()
        },
        scopes: rules
            .iter()
            .map(|(scope, rgb)| ThemeItem {
                scope: ScopeSelectors::from_str(scope).expect("a valid scope selector"),
                style: StyleModifier {
                    // Pushed to 3:1 on the background, hue kept (a light theme's bright-black
                    // comment would otherwise vanish).
                    foreground: Some(color(ensure_contrast(*rgb, theme.background, MARK_MIN))),
                    ..StyleModifier::default()
                },
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::parse;
    use crate::theme::{apply, builtin_theme};
    use std::collections::HashSet;
    use syntect::parsing::Scope;

    const MB: u64 = 1024 * 1024;

    fn named(name: &str) -> Theme {
        let mut t = Theme::default();
        apply(&mut t, &parse(builtin_theme(name).unwrap()));
        t
    }

    #[test]
    fn classify_size_boundaries() {
        assert_eq!(classify(2 * MB, b"x"), Viewable::Text);
        assert_eq!(classify(2 * MB + 1, b"x"), Viewable::PlainTooLarge);
        assert_eq!(classify(20 * MB, b"x"), Viewable::PlainTooLarge);
        assert_eq!(classify(20 * MB + 1, b"x"), Viewable::Refuse);
        assert_eq!(classify(0, b""), Viewable::Text);
    }

    #[test]
    fn classify_nul_in_head_refuses() {
        assert_eq!(classify(10, b"\0abc"), Viewable::Refuse);
        let mut head = vec![b'a'; 8192];
        head[8191] = 0;
        assert_eq!(classify(8192, &head), Viewable::Refuse);
        assert_eq!(classify(8192, &vec![b'a'; 8192]), Viewable::Text);
    }

    #[test]
    fn rust_snippet_has_several_colours_and_covers_every_byte() {
        let theme = named("Catppuccin Mocha");
        let h = Highlighter::load(&theme);
        let text = "// hi\nfn main() {\n    let s = \"x\";\n    let n = 42;\n}\n";
        let lines = h.colour(Path::new("a.rs"), text).spans;
        let mut colours = HashSet::new();
        for (spans, line) in lines.iter().zip(text.split_inclusive('\n')) {
            let mut at = 0;
            for (r, c) in spans {
                assert_eq!(r.start, at);
                at = r.end;
                colours.insert((c.r, c.g, c.b));
            }
            assert_eq!(at, line.len());
        }
        assert!(colours.len() >= 3, "{colours:?}");
    }

    #[test]
    fn unknown_extension_is_one_foreground_span_per_line() {
        let theme = named("Catppuccin Mocha");
        let h = Highlighter::load(&theme);
        let lines = h.colour(Path::new("a.zzzzz"), "one two\nthree\n").spans;
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], vec![(0..8, theme.foreground)]);
        assert_eq!(lines[1], vec![(0..6, theme.foreground)]);
    }

    #[test]
    fn empty_file_is_one_empty_line() {
        let h = Highlighter::load(&Theme::default());
        let lines = h.colour(Path::new("a.rs"), "").spans;
        assert_eq!(lines.len(), 1);
        assert!(lines[0].is_empty());
    }

    #[test]
    fn syntax_name_follows_the_highlight_lookup() {
        let h = Highlighter::load(&Theme::default());
        assert_eq!(h.syntax_name(Path::new("a.rs"), ""), "Rust");
        assert_eq!(h.syntax_name(Path::new("Makefile"), ""), "Makefile");
        assert_eq!(
            h.syntax_name(Path::new("x"), "#!/bin/bash"),
            "Bourne Again Shell (bash)"
        );
        assert_eq!(h.syntax_name(Path::new("x.zzz"), ""), "Plain Text");
    }

    fn style_fg(st: &syntect::highlighting::Theme, scope: &str) -> Rgb {
        let h = syntect::highlighting::Highlighter::new(st);
        let c = h.style_for_stack(&[Scope::new(scope).unwrap()]).foreground;
        Rgb {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }

    #[test]
    fn strings_and_comments_take_the_fuxx_colours_pushed_to_3_to_1() {
        for name in ["Catppuccin Mocha", "Catppuccin Latte"] {
            let theme = named(name);
            let st = syntect_theme(&theme);
            let string = style_fg(&st, "string.quoted.double");
            let comment = style_fg(&st, "comment.line");
            assert_eq!(
                string,
                ensure_contrast(theme.palette[2], theme.background, MARK_MIN),
                "{name}"
            );
            assert_eq!(
                comment,
                ensure_contrast(theme.palette[8], theme.background, MARK_MIN),
                "{name}"
            );
            assert_ne!(style_fg(&st, "source"), string);
        }
    }

    #[test]
    fn every_rule_colour_reaches_3_to_1_on_the_background() {
        for name in ["Catppuccin Mocha", "Catppuccin Latte"] {
            let theme = named(name);
            for item in &syntect_theme(&theme).scopes {
                let c = item.style.foreground.unwrap();
                let rgb = Rgb {
                    r: c.r,
                    g: c.g,
                    b: c.b,
                };
                let ratio = crate::ui_tokens::contrast(rgb, theme.background);
                assert!(ratio >= MARK_MIN - 0.01, "{name}: {rgb:?} {ratio}");
            }
        }
    }

    #[test]
    fn with_theme_recolours_and_keeps_the_syntaxes() {
        let (mocha, latte) = (named("Catppuccin Mocha"), named("Catppuccin Latte"));
        let dark = Highlighter::load(&mocha);
        let light = dark.with_theme(&latte);
        let text = "let s = \"x\";\n";
        let string = |h: &Highlighter| {
            let lines = h.colour(Path::new("a.rs"), text).spans;
            let at = text.find('"').unwrap();
            lines[0].iter().find(|(r, _)| r.contains(&at)).unwrap().1
        };
        assert_eq!(
            string(&light),
            ensure_contrast(latte.palette[2], latte.background, MARK_MIN)
        );
        assert_eq!(
            string(&dark),
            ensure_contrast(mocha.palette[2], mocha.background, MARK_MIN)
        );
    }

    #[test]
    fn a_file_without_an_extension_is_found_by_name() {
        let h = Highlighter::load(&named("Catppuccin Mocha"));
        let text = "all:\n\techo hi\n";
        let lines = h.colour(Path::new("Makefile"), text).spans;
        let mut colours = HashSet::new();
        for spans in &lines {
            for (_, c) in spans {
                colours.insert((c.r, c.g, c.b));
            }
        }
        assert!(colours.len() >= 2, "{colours:?}");
    }

    /// M11ai: a fence's language picks the syntax (`find_syntax_by_token`: an extension or a
    /// name, any case).
    #[test]
    fn the_fence_s_first_word_is_the_language() {
        let theme = named("Catppuccin Mocha");
        let h = Highlighter::load(&theme);
        let keyword = ensure_contrast(theme.palette[5], theme.background, MARK_MIN);
        for lang in ["rust", "RUST", "rs"] {
            let lines = h.highlight_lang(lang, "fn main() {}").expect(lang);
            assert_eq!(lines.len(), 1, "{lang}");
            let fn_colour = lines[0].iter().find(|(r, _)| r.contains(&0)).unwrap().1;
            assert_eq!(fn_colour, keyword, "{lang}: `fn` in the keyword colour");
        }
        for (lang, text) in [
            ("ts", "export const port = 8080;\n"),
            ("sh", "cd ~/code/my-app && ./install.sh --prefix ~/.local\n"),
        ] {
            let lines = h.highlight_lang(lang, text).expect(lang);
            assert!(
                lines.iter().flatten().any(|(_, c)| *c != theme.foreground),
                "{lang}: some colour besides the text's: {lines:?}"
            );
        }
    }

    #[test]
    fn an_unknown_language_is_plain() {
        let h = Highlighter::load(&named("Catppuccin Mocha"));
        assert!(h.highlight_lang("foo", "fn main() {}").is_none());
    }

    #[test]
    fn named_lines_come_with_the_colours() {
        let h = Highlighter::load(&named("Catppuccin Mocha"));
        let text = "impl S {\n    fn r() {\n        x();\n    }\n}\n";
        let coloured = h.colour(Path::new("a.rs"), text);
        assert_eq!(coloured.named, vec![0, 1]);
        assert_eq!(coloured.spans.len(), 5);
        // Markdown headings are named; a `#` line inside a fence is a comment, not a heading.
        let md = h.colour(
            Path::new("a.md"),
            "# Setup\n\n```sh\n# install\nmake\n```\n\n## Next\n",
        );
        assert_eq!(md.named, vec![0, 7]);
        // No syntax, no text: nothing named.
        assert!(
            h.colour(Path::new("a.zzzzz"), "fn r() {}\n")
                .named
                .is_empty()
        );
        assert!(h.colour(Path::new("a.rs"), "").named.is_empty());
    }
}
