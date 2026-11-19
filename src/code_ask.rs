//! Code references for "Ask the agent" (M11ag): what a file-tab selection becomes as a chip
//! label and as the text the agent (or the clipboard) gets. Pure: no GPUI, no I/O.

#![allow(dead_code)] // wired in by the later M11ag tasks

use std::path::{Path, PathBuf};

use crate::file_tab::{Doc, Pos};
use crate::md_reader;

/// A reference is too long to ask past this many lines ...
pub const ASK_MAX_LINES: usize = 500;
/// ... or this many bytes.
pub const ASK_MAX_BYTES: usize = 64 * 1024;

/// Which lines a reference names (1-based, inclusive). `Removed` names lines of the old text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodeLines {
    Lines { first: usize, last: usize },
    Removed { first: usize, last: usize },
}

/// Where Ask goes: the agent named on the bar, or nowhere (copy only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskTo {
    Agent(&'static str),
    CopyOnly,
}

impl Default for AskTo {
    fn default() -> Self {
        AskTo::Agent("claude")
    }
}

/// Some code and where it is. `path` is as the tab shows it, `real` the file it stands for when
/// that differs (a link); `body` has no `\r` and keeps its tabs.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeRef {
    pub path: PathBuf,
    pub real: Option<PathBuf>,
    pub lines: CodeLines,
    pub fence: Option<String>,
    pub body: String,
}

impl CodeRef {
    /// The chip's label: the file name and the lines, with an en dash.
    pub fn label(&self) -> String {
        let name = self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        match self.lines {
            CodeLines::Lines { first, last } if first == last => format!("{name}:{first}"),
            CodeLines::Lines { first, last } => format!("{name}:{first}\u{2013}{last}"),
            CodeLines::Removed { first, last } if first == last => {
                format!("{name} (removed, was {first})")
            }
            CodeLines::Removed { first, last } => {
                format!("{name} (removed, was {first}\u{2013}{last})")
            }
        }
    }

    /// The text's first line: `shown` and the lines, with an ASCII hyphen.
    pub fn header(&self, shown: &str) -> String {
        match self.lines {
            CodeLines::Lines { first, last } if first == last => format!("{shown}:{first}"),
            CodeLines::Lines { first, last } => format!("{shown}:{first}-{last}"),
            CodeLines::Removed { first, last } if first == last => {
                format!("{shown} (removed, was {first})")
            }
            CodeLines::Removed { first, last } => {
                format!("{shown} (removed, was {first}-{last})")
            }
        }
    }

    /// What the agent receives: the header, then the code in a fence.
    pub fn text(&self, shown: &str) -> String {
        format!(
            "{}\n```{}\n{}\n```",
            self.header(shown),
            self.fence.as_deref().unwrap_or(""),
            self.body
        )
    }

    /// Past the cap: more than `ASK_MAX_LINES` lines or `ASK_MAX_BYTES` bytes.
    pub fn too_long(&self) -> bool {
        self.body.lines().count() > ASK_MAX_LINES || self.body.len() > ASK_MAX_BYTES
    }

    /// The same file and lines (what makes two chips one).
    pub fn same_place(&self, other: &CodeRef) -> bool {
        self.path == other.path && self.lines == other.lines
    }
}

/// The 0-based, inclusive lines a selection from `a` to `b` touches (either order), clamped to
/// `last`. A selection ending at column 0 of a line still takes that line.
pub fn whole_lines(a: Pos, b: Pos, last: usize) -> (usize, usize) {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    (lo.line.min(last), hi.line.min(last))
}

/// The reference for a selection in a file tab's text; None when it is empty. Every line from
/// the first to the last is taken, a folded stretch's hidden lines too.
pub fn file_ref(path: &Path, real: Option<&Path>, doc: &Doc, a: Pos, b: Pos) -> Option<CodeRef> {
    if a == b {
        return None;
    }
    let (first, last) = whole_lines(a, b, doc.lines.len().saturating_sub(1));
    let body = (first..=last)
        .map(|i| doc.line(i))
        .collect::<Vec<_>>()
        .join("\n");
    Some(CodeRef {
        path: path.to_path_buf(),
        real: real.map(Path::to_path_buf),
        lines: CodeLines::Lines {
            first: first + 1,
            last: last + 1,
        },
        fence: fence_lang(doc.syntax.as_deref(), path),
        body,
    })
}

/// The fence's language: the syntax's name lowercased when it is one word of letters, digits,
/// `+` or `#`; else the extension lowercased; none for plain text or no extension.
pub fn fence_lang(syntax: Option<&str>, path: &Path) -> Option<String> {
    if syntax == Some("Plain Text") {
        return None;
    }
    if let Some(name) = syntax
        && !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '+' | '#'))
    {
        return Some(name.to_lowercase());
    }
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    (!ext.is_empty()).then_some(ext)
}

/// The path the agent is told: relative to `cwd` when `path` (else `real`) is strictly inside
/// it, else `path` whole.
pub fn agent_path(path: &Path, real: Option<&Path>, cwd: &Path) -> String {
    let inside = |p: &Path| {
        p.strip_prefix(cwd)
            .ok()
            .filter(|rest| !rest.as_os_str().is_empty())
            .map(|rest| rest.display().to_string())
    };
    inside(path)
        .or_else(|| real.and_then(inside))
        .unwrap_or_else(|| path.display().to_string())
}

/// The copied text: the reference with the path as `md_reader::shown_path` shows it.
pub fn copy_text(code: &CodeRef, rel: Option<&Path>, home: Option<&Path>) -> String {
    code.text(&md_reader::shown_path(rel, &code.path, home))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(n: usize) -> Doc {
        let text = (1..=n)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        Doc::new(text, 0)
    }

    fn code(first: usize, last: usize) -> CodeRef {
        CodeRef {
            path: PathBuf::from("/p/fuxx-cli/src/workspace.rs"),
            real: None,
            lines: CodeLines::Lines { first, last },
            fence: Some("rust".into()),
            body: "a".into(),
        }
    }

    fn p(line: usize, col: usize) -> Pos {
        Pos { line, col }
    }

    #[test]
    fn labels_and_headers() {
        let c = code(43, 45);
        assert_eq!(c.label(), "workspace.rs:43\u{2013}45");
        assert_eq!(c.header("src/workspace.rs"), "src/workspace.rs:43-45");
        let one = code(43, 43);
        assert_eq!(one.label(), "workspace.rs:43");
        assert_eq!(one.header("src/workspace.rs"), "src/workspace.rs:43");
        let mut gone = code(1, 1);
        gone.lines = CodeLines::Removed {
            first: 12,
            last: 14,
        };
        assert_eq!(gone.label(), "workspace.rs (removed, was 12\u{2013}14)");
        assert_eq!(
            gone.header("src/workspace.rs"),
            "src/workspace.rs (removed, was 12-14)"
        );
        gone.lines = CodeLines::Removed {
            first: 12,
            last: 12,
        };
        assert_eq!(gone.label(), "workspace.rs (removed, was 12)");
    }

    #[test]
    fn the_text_is_header_then_fence() {
        let mut c = code(43, 45);
        c.body = "if let Some(tab) = pane.find(&path) {\n    return self.show(tab);\n}".into();
        assert_eq!(
            c.text("src/workspace.rs"),
            "src/workspace.rs:43-45\n```rust\nif let Some(tab) = pane.find(&path) {\n    return self.show(tab);\n}\n```"
        );
    }

    #[test]
    fn whole_lines_take_every_line_touched() {
        assert_eq!(whole_lines(p(42, 7), p(44, 2), 99), (42, 44));
        assert_eq!(whole_lines(p(44, 2), p(42, 7), 99), (42, 44));
        assert_eq!(whole_lines(p(42, 7), p(500, 0), 99), (42, 99));
    }

    #[test]
    fn a_selection_ending_at_column_zero_takes_that_line() {
        let d = doc(10);
        let c = file_ref(Path::new("a.txt"), None, &d, p(2, 3), p(4, 0)).unwrap();
        assert_eq!(c.lines, CodeLines::Lines { first: 3, last: 5 });
        assert_eq!(c.body, "line 3\nline 4\nline 5");
    }

    #[test]
    fn a_fold_inside_the_selection_is_taken() {
        let d = doc(30);
        let c = file_ref(Path::new("a.txt"), None, &d, p(10, 0), p(20, 1)).unwrap();
        assert_eq!(c.body.lines().count(), 11);
        assert_eq!(
            c.lines,
            CodeLines::Lines {
                first: 11,
                last: 21
            }
        );
    }

    #[test]
    fn an_empty_selection_has_no_reference() {
        let d = doc(3);
        assert!(file_ref(Path::new("a.txt"), None, &d, p(1, 2), p(1, 2)).is_none());
    }

    #[test]
    fn crlf_lines_lose_their_cr_and_keep_tabs() {
        let d = Doc::new("one\r\n\ttwo\r\nthree\r\n".into(), 0);
        let c = file_ref(Path::new("/tmp/x/Makefile"), None, &d, p(0, 0), p(1, 2)).unwrap();
        assert_eq!(c.body, "one\n\ttwo");
        assert!(!c.body.contains('\r'));
    }

    #[test]
    fn the_cap_is_500_lines_or_64_kb() {
        let mut c = code(1, 500);
        c.body = vec!["x"; 500].join("\n");
        assert!(!c.too_long());
        c.body = vec!["x"; 501].join("\n");
        assert!(c.too_long());
        c.body = vec!["y".repeat(300); 300].join("\n");
        assert!(c.too_long());
    }

    #[test]
    fn fence_lang_from_the_syntax() {
        let f = |s: Option<&str>, p: &str| fence_lang(s, Path::new(p));
        assert_eq!(f(Some("Rust"), "a.rs").as_deref(), Some("rust"));
        assert_eq!(f(Some("C++"), "a.cpp").as_deref(), Some("c++"));
        assert_eq!(
            f(Some("Bourne Again Shell (bash)"), "x.sh").as_deref(),
            Some("sh")
        );
        assert_eq!(f(Some("Plain Text"), "x.zzz"), None);
        assert_eq!(f(None, "Makefile"), None);
        assert_eq!(f(None, "a.rs").as_deref(), Some("rs"));
        assert_eq!(f(Some("Makefile"), "Makefile").as_deref(), Some("makefile"));
    }

    #[test]
    fn paths_relative_inside_the_folder_else_absolute() {
        let cwd = Path::new("/p/fuxx-cli");
        let inside = Path::new("/p/fuxx-cli/src/workspace.rs");
        assert_eq!(agent_path(inside, None, cwd), "src/workspace.rs");
        let outside = Path::new("/q/other/src/workspace.rs");
        assert_eq!(agent_path(outside, None, cwd), "/q/other/src/workspace.rs");
        assert_eq!(agent_path(outside, Some(inside), cwd), "src/workspace.rs");
        assert_eq!(agent_path(cwd, None, cwd), "/p/fuxx-cli");
    }

    #[test]
    fn copy_text_uses_the_repo_relative_path() {
        let c = code(43, 45);
        assert!(
            copy_text(&c, Some(Path::new("src/workspace.rs")), None)
                .starts_with("src/workspace.rs:43-45\n")
        );
        assert!(
            copy_text(&c, None, Some(Path::new("/p")))
                .starts_with("~/fuxx-cli/src/workspace.rs:43-45\n")
        );
        assert!(copy_text(&c, None, None).starts_with("/p/fuxx-cli/src/workspace.rs:43-45\n"));
    }

    #[test]
    fn same_place_is_path_and_lines() {
        let a = code(1, 2);
        let mut b = code(1, 2);
        b.body = "other".into();
        assert!(a.same_place(&b));
        assert!(!a.same_place(&code(1, 3)));
        let mut c = code(1, 2);
        c.path = PathBuf::from("/p/fuxx-cli/src/file_tab.rs");
        assert!(!a.same_place(&c));
    }
}
