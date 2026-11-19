//! Code references for "Ask the agent" (M11ag): what a file-tab selection becomes as a chip
//! label and as the text the agent (or the clipboard) gets. Pure: no GPUI, no I/O.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::diff_view::{DiffRow, RowKind};
use crate::file_tab::{Doc, Pos};
use crate::md_reader;
use crate::row_map::Half;

/// A reference is too long to ask past this many lines ...
pub const ASK_MAX_LINES: usize = 500;
/// ... or this many bytes.
pub const ASK_MAX_BYTES: usize = 64 * 1024;

/// Whether a body of `lines` lines and `bytes` bytes is past the cap.
pub fn too_long_for(lines: usize, bytes: usize) -> bool {
    lines > ASK_MAX_LINES || bytes > ASK_MAX_BYTES
}

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

    /// What the agent receives: the header, then the code in a fence: one backtick longer than
    /// the longest run of backticks in the code, and never under three, so code with a fence
    /// of its own (Markdown) does not close it early.
    pub fn text(&self, shown: &str) -> String {
        let (mut longest, mut run) = (0, 0);
        for c in self.body.chars() {
            run = if c == '`' { run + 1 } else { 0 };
            longest = longest.max(run);
        }
        let fence = "`".repeat((longest + 1).max(3));
        format!(
            "{}\n{fence}{}\n{}\n{fence}",
            self.header(shown),
            self.fence.as_deref().unwrap_or(""),
            self.body
        )
    }

    /// Past the cap: more than `ASK_MAX_LINES` lines or `ASK_MAX_BYTES` bytes.
    pub fn too_long(&self) -> bool {
        // `split`, not `lines()`: a blank last line is a line.
        too_long_for(self.body.split('\n').count(), self.body.len())
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

/// The diff rows a selection over rows `first..=last` takes: those on `half`'s side (side by
/// side; both sides unified), not a Gap, not in a held (folded or closed) stretch.
pub fn diff_rows_kept<'a>(
    rows: &'a [DiffRow],
    held: &'a [Range<usize>],
    half: Option<Half>,
    first: usize,
    last: usize,
) -> impl Iterator<Item = usize> + 'a {
    let on = move |r: &DiffRow| match r.kind {
        RowKind::Same => true,
        RowKind::Removed => half != Some(Half::New),
        RowKind::Added => half != Some(Half::Old),
        RowKind::Gap => false,
    };
    let last = last.min(rows.len().saturating_sub(1));
    (first..=last)
        .filter(move |&i| rows.get(i).is_some_and(on) && !held.iter().any(|h| h.contains(&i)))
}

/// The reference for a selection in Diff mode (unified, or one `half` side by side); None when
/// no row is kept.
#[allow(clippy::too_many_arguments)]
pub fn diff_ref(
    path: &Path,
    real: Option<&Path>,
    doc: &Doc,
    rows: &[DiffRow],
    held: &[Range<usize>],
    half: Option<Half>,
    a: Pos,
    b: Pos,
) -> Option<CodeRef> {
    let (first, last) = whole_lines(a, b, rows.len().saturating_sub(1));
    let kept: Vec<usize> = diff_rows_kept(rows, held, half, first, last).collect();
    let marker = |kind| match kind {
        RowKind::Added => '+',
        RowKind::Removed => '-',
        _ => ' ',
    };
    let body = kept
        .iter()
        .map(|&i| format!("{}{}", marker(rows[i].kind), doc.line(i)))
        .collect::<Vec<_>>()
        .join("\n");
    let span = |side: fn(&DiffRow) -> Option<usize>| {
        let mut nums = kept.iter().filter_map(|&i| side(&rows[i]));
        let first = nums.next()?;
        let (lo, hi) = nums.fold((first, first), |(lo, hi), n| (lo.min(n), hi.max(n)));
        Some((lo, hi))
    };
    let lines = match (span(|r| r.new), span(|r| r.old)) {
        (Some((first, last)), _) => CodeLines::Lines { first, last },
        (None, Some((first, last))) => CodeLines::Removed { first, last },
        (None, None) => return None,
    };
    Some(CodeRef {
        path: path.to_path_buf(),
        real: real.map(Path::to_path_buf),
        lines,
        fence: Some("diff".into()),
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

    /// Final fixes, F4: the fence is one backtick longer than the longest run of backticks in
    /// the code (never under three), so a fence inside the code does not close it early.
    #[test]
    fn the_fence_is_longer_than_any_backtick_run_in_the_code() {
        let mut c = code(1, 5);
        c.fence = Some("markdown".into());
        c.body = "Run it:\n```sh\ncargo test --bin fuxx\n```\nDone.".into();
        assert_eq!(
            c.text("docs/README.md"),
            "docs/README.md:1-5\n````markdown\nRun it:\n```sh\ncargo test --bin fuxx\n```\nDone.\n````"
        );
        // A longer run inside a line counts too; one or two backticks leave the fence at three.
        c.body = "a ````` b".into();
        assert!(
            c.text("x.md")
                .ends_with("\n``````markdown\na ````` b\n``````")
        );
        c.body = "let s = `x`; ``y``".into();
        c.fence = None;
        assert_eq!(c.text("x.md"), "x.md:1-5\n```\nlet s = `x`; ``y``\n```");
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
    fn a_body_ending_in_a_blank_line_counts_it() {
        // 500 lines and a blank 501st: `lines()` would say 500.
        let mut c = code(1, 501);
        c.body = format!("{}\n", vec!["x"; 500].join("\n"));
        assert_eq!(c.body.split('\n').count(), 501);
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

    fn drow(kind: RowKind, old: Option<usize>, new: Option<usize>) -> DiffRow {
        DiffRow { kind, old, new }
    }

    fn diff_doc(texts: &[&str]) -> Doc {
        Doc::new(texts.join("\n"), 0)
    }

    fn sample() -> (Doc, Vec<DiffRow>) {
        (
            diff_doc(&["a", "b", "c", "d"]),
            vec![
                drow(RowKind::Same, Some(41), Some(43)),
                drow(RowKind::Removed, Some(42), None),
                drow(RowKind::Added, None, Some(44)),
                drow(RowKind::Same, Some(43), Some(45)),
            ],
        )
    }

    #[test]
    fn diff_ref_names_the_current_lines() {
        let (d, rows) = sample();
        let c = diff_ref(
            Path::new("a.rs"),
            None,
            &d,
            &rows,
            &[],
            None,
            p(0, 0),
            p(3, 1),
        )
        .unwrap();
        assert_eq!(
            c.lines,
            CodeLines::Lines {
                first: 43,
                last: 45
            }
        );
        assert_eq!(c.body, " a\n-b\n+c\n d");
        assert_eq!(c.fence.as_deref(), Some("diff"));
    }

    #[test]
    fn removed_only_is_named_after_the_old_lines() {
        let d = diff_doc(&["x", "y", "z"]);
        let rows = vec![
            drow(RowKind::Removed, Some(12), None),
            drow(RowKind::Removed, Some(13), None),
            drow(RowKind::Removed, Some(14), None),
        ];
        let path = Path::new("/p/fuxx-cli/src/workspace.rs");
        let c = diff_ref(path, None, &d, &rows, &[], None, p(0, 0), p(2, 1)).unwrap();
        assert_eq!(
            c.lines,
            CodeLines::Removed {
                first: 12,
                last: 14
            }
        );
        assert_eq!(c.label(), "workspace.rs (removed, was 12\u{2013}14)");
        assert_eq!(c.body, "-x\n-y\n-z");
    }

    #[test]
    fn held_and_gap_rows_are_left_out() {
        let d = diff_doc(&["a", "", "h1", "h2", "b"]);
        let rows = vec![
            drow(RowKind::Same, Some(1), Some(1)),
            drow(RowKind::Gap, None, None),
            drow(RowKind::Same, Some(2), Some(2)),
            drow(RowKind::Same, Some(3), Some(3)),
            drow(RowKind::Added, None, Some(4)),
        ];
        let held: Vec<Range<usize>> = std::iter::once(2..4).collect();
        let c = diff_ref(
            Path::new("a.rs"),
            None,
            &d,
            &rows,
            &held,
            None,
            p(0, 0),
            p(4, 1),
        )
        .unwrap();
        assert_eq!(c.body, " a\n+b");
        assert_eq!(c.lines, CodeLines::Lines { first: 1, last: 4 });
        // Only a Gap and held lines: nothing to ask about.
        assert!(
            diff_ref(
                Path::new("a.rs"),
                None,
                &d,
                &rows,
                &held,
                None,
                p(1, 0),
                p(3, 1)
            )
            .is_none()
        );
    }

    #[test]
    fn side_by_side_takes_only_its_half() {
        let (d, rows) = sample();
        let path = Path::new("a.rs");
        let old = diff_ref(
            path,
            None,
            &d,
            &rows,
            &[],
            Some(Half::Old),
            p(0, 0),
            p(3, 1),
        )
        .unwrap();
        assert_eq!(old.body, " a\n-b\n d");
        assert_eq!(
            old.lines,
            CodeLines::Lines {
                first: 43,
                last: 45
            }
        );
        let new = diff_ref(
            path,
            None,
            &d,
            &rows,
            &[],
            Some(Half::New),
            p(0, 0),
            p(3, 1),
        )
        .unwrap();
        assert_eq!(new.body, " a\n+c\n d");
        // The old half's removed line alone: named after the old lines.
        let gone = diff_ref(
            path,
            None,
            &d,
            &rows,
            &[],
            Some(Half::Old),
            p(1, 0),
            p(2, 1),
        )
        .unwrap();
        assert_eq!(
            gone.lines,
            CodeLines::Removed {
                first: 42,
                last: 42
            }
        );
    }
}
