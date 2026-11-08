//! Syntax colours for read-only file tabs (M11g): file-size limits and a syntect highlighter whose
//! theme is built from the fuxx terminal theme, so file colours follow the chosen theme.

use crate::theme::Theme;
use crate::ui_tokens::{MARK_MIN, ensure_contrast};
use alacritty_terminal::vte::ansi::Rgb;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, ScopeSelectors, StyleModifier, ThemeItem, ThemeSettings};
use syntect::parsing::SyntaxSet;

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

/// A byte range within one line, and its colour.
pub type Span = (std::ops::Range<usize>, Rgb);

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

    /// One `Vec` per line (a line keeps its `\n`, so the ranges cover every byte). The syntax is
    /// found by file extension, then by the first line; otherwise the text is plain. An empty text
    /// is one empty line.
    pub fn highlight(&self, path: &Path, text: &str) -> Vec<Vec<Span>> {
        if text.is_empty() {
            return vec![Vec::new()];
        }
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
        let by_ext = by_ext.or_else(by_name);
        let first_line = text.split_inclusive('\n').next().unwrap_or("");
        let syntax = by_ext.or_else(|| self.syntaxes.find_syntax_by_first_line(first_line));
        let Some(syntax) = syntax else {
            return text
                .split_inclusive('\n')
                .map(|l| vec![(0..l.len(), self.foreground)])
                .collect();
        };
        let mut hl = HighlightLines::new(syntax, &self.theme);
        text.split_inclusive('\n')
            .map(|line| match hl.highlight_line(line, &self.syntaxes) {
                Ok(parts) => {
                    let mut spans: Vec<Span> = Vec::with_capacity(parts.len());
                    let mut at = 0;
                    for (style, piece) in parts {
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
                    spans
                }
                // A parse error: show the line plain.
                Err(_) => vec![(0..line.len(), self.foreground)],
            })
            .collect()
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
        let lines = h.highlight(Path::new("a.rs"), text);
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
        let lines = h.highlight(Path::new("a.zzzzz"), "one two\nthree\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], vec![(0..8, theme.foreground)]);
        assert_eq!(lines[1], vec![(0..6, theme.foreground)]);
    }

    #[test]
    fn empty_file_is_one_empty_line() {
        let h = Highlighter::load(&Theme::default());
        let lines = h.highlight(Path::new("a.rs"), "");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].is_empty());
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
            let lines = h.highlight(Path::new("a.rs"), text);
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
        let lines = h.highlight(Path::new("Makefile"), text);
        let mut colours = HashSet::new();
        for spans in &lines {
            for (_, c) in spans {
                colours.insert((c.r, c.g, c.b));
            }
        }
        assert!(colours.len() >= 2, "{colours:?}");
    }
}
