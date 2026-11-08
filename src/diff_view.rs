//! A file's whole diff against the last commit as rows (M11h): the unified diff parsed into
//! numbered rows (with a gap row between hunks), the two sides rebuilt for highlighting, and the
//! dimmed colour of unchanged rows. Pure: git runs in `git_status::file_diff`; the file tab draws.

use alacritty_terminal::vte::ansi::Rgb;

use crate::git_status::{FileDiff, LineChange};
use crate::highlight::Span;
use crate::theme::mix;
use crate::ui_tokens::{MARK_MIN, TEXT_MIN, contrast, ensure_contrast};

/// What a row is: unchanged, added, removed, or the gap between two hunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Same,
    Added,
    Removed,
    Gap,
}

/// One row of the diff; the numbers are 1-based line numbers on each side (a Gap has neither).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffRow {
    pub kind: RowKind,
    pub old: Option<usize>,
    pub new: Option<usize>,
}

/// A parsed diff: `text` is one line per row (`\n`-joined, no `+`/`-` prefix, a Gap's line empty).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub text: String,
    pub rows: Vec<DiffRow>,
}

/// What to show for a file's diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Rows(Parsed),
    /// Nothing changed (or nothing to show).
    Same,
    Binary,
}

/// Fraction of the way unchanged rows' colours move toward the background.
pub const DIM_MIX: f32 = 0.35;
pub const NO_CHANGES: &str = "No changes since the last commit.";
pub const BINARY: &str = "This file is binary: no diff to show.";
pub const NO_REPO: &str = "Not a git repository.";

/// The context lines to ask git for: every line of the file, plus one (a missing file: 1), so
/// the diff is the whole file.
pub fn context_for(disk_lines: usize) -> usize {
    disk_lines.saturating_add(1)
}

/// `-a[,b]`'s or `+c[,d]`'s start number.
fn hunk_start(part: &str) -> Option<usize> {
    let part = part.get(1..)?;
    part.split(',').next()?.parse().ok()
}

/// A unified diff as rows. Everything before the first `@@` (the `diff --git`, `index`, `---`
/// and `+++` lines) is skipped; after it every line is a row by its first character (` `, `-`,
/// `+`; a `\ No newline` marker is skipped), so a `--- x` line inside a hunk is a removed `-- x`.
/// A Gap row separates two hunks. A line's trailing CR is dropped.
pub fn parse_diff(unified: &str) -> Parsed {
    let mut lines: Vec<&str> = Vec::new();
    let mut rows: Vec<DiffRow> = Vec::new();
    let (mut old, mut new) = (1, 1);
    let mut started = false;
    for line in unified.lines() {
        if line.starts_with("@@ ") {
            let mut parts = line.split(' ').skip(1);
            let (Some(a), Some(c)) = (
                parts.next().and_then(hunk_start),
                parts.next().and_then(hunk_start),
            ) else {
                continue;
            };
            if started {
                lines.push("");
                rows.push(DiffRow {
                    kind: RowKind::Gap,
                    old: None,
                    new: None,
                });
            }
            started = true;
            (old, new) = (a, c);
            continue;
        }
        if !started {
            continue;
        }
        // `diff.suppressBlankEmpty` prints a blank context line as an empty line.
        let first = line.chars().next().unwrap_or(' ');
        let text = &line[first.len_utf8().min(line.len())..];
        let text = text.strip_suffix('\r').unwrap_or(text);
        let row = match first {
            ' ' => DiffRow {
                kind: RowKind::Same,
                old: Some(old),
                new: Some(new),
            },
            '-' => DiffRow {
                kind: RowKind::Removed,
                old: Some(old),
                new: None,
            },
            '+' => DiffRow {
                kind: RowKind::Added,
                old: None,
                new: Some(new),
            },
            _ => continue,
        };
        if row.old.is_some() {
            old += 1;
        }
        if row.new.is_some() {
            new += 1;
        }
        lines.push(text);
        rows.push(row);
    }
    Parsed {
        text: lines.join("\n"),
        rows,
    }
}

/// Every line of `text` as an Added row, numbered from 1 (a new file).
pub fn all_added(text: &str) -> Parsed {
    let lines: Vec<&str> = text.lines().collect();
    Parsed {
        text: lines.join("\n"),
        rows: (1..=lines.len())
            .map(|n| DiffRow {
                kind: RowKind::Added,
                old: None,
                new: Some(n),
            })
            .collect(),
    }
}

/// What git's answer shows: `Unified` with no rows is `Same`; `AllAdded` shows `disk_text`
/// (none, or empty: nothing to show, `Same`).
pub fn diff_outcome(diff: FileDiff, disk_text: Option<&str>) -> Outcome {
    let rows = |p: Parsed| {
        if p.rows.is_empty() {
            Outcome::Same
        } else {
            Outcome::Rows(p)
        }
    };
    match diff {
        FileDiff::Unified(u) => rows(parse_diff(&u)),
        FileDiff::AllAdded => disk_text.map_or(Outcome::Same, |t| rows(all_added(t))),
        FileDiff::Binary => Outcome::Binary,
    }
}

/// The old text (Same and Removed rows) and the new text (Same and Added), each line ending in
/// `\n`, for highlighting each side as a file.
pub fn sides(p: &Parsed) -> (String, String) {
    let (mut old, mut new) = (String::new(), String::new());
    for (line, row) in p.text.split('\n').zip(&p.rows) {
        let (on_old, on_new) = match row.kind {
            RowKind::Same => (true, true),
            RowKind::Removed => (true, false),
            RowKind::Added => (false, true),
            RowKind::Gap => (false, false),
        };
        if on_old {
            old.push_str(line);
            old.push('\n');
        }
        if on_new {
            new.push_str(line);
            new.push('\n');
        }
    }
    (old, new)
}

/// Each row's colour spans: a Removed row's from `old`, a Same or Added row's from `new` (by
/// line number); a Gap's, or a line out of range, none.
pub fn diff_spans(rows: &[DiffRow], old: &[Vec<Span>], new: &[Vec<Span>]) -> Vec<Vec<Span>> {
    rows.iter()
        .map(|row| {
            let (side, n) = match row.kind {
                RowKind::Removed => (old, row.old),
                RowKind::Same | RowKind::Added => (new, row.new),
                RowKind::Gap => return Vec::new(),
            };
            n.and_then(|n| side.get(n.checked_sub(1)?))
                .cloned()
                .unwrap_or_default()
        })
        .collect()
}

/// The first Added or Removed row.
pub fn first_change(rows: &[DiffRow]) -> Option<usize> {
    rows.iter()
        .position(|r| matches!(r.kind, RowKind::Added | RowKind::Removed))
}

/// The row to scroll to for a change bar's line: an Added or Changed line n is the row with
/// new number n, moved back over the Removed rows right before it; a deletion after line c is
/// the row after the row with new number c (c = 0: row 0). None when the line isn't there.
pub fn reveal_row(rows: &[DiffRow], change: (usize, LineChange)) -> Option<usize> {
    let (n, kind) = change;
    match kind {
        LineChange::Added | LineChange::Changed => {
            let mut at = rows.iter().position(|r| r.new == Some(n))?;
            while at > 0 && rows[at - 1].kind == RowKind::Removed {
                at -= 1;
            }
            Some(at)
        }
        LineChange::DeletedAfter if n == 0 => (!rows.is_empty()).then_some(0),
        LineChange::DeletedAfter => {
            let at = rows.iter().position(|r| r.new == Some(n))? + 1;
            (at < rows.len()).then_some(at)
        }
    }
}

/// An unchanged row's colour: `c` mixed `DIM_MIX` toward `background`, held at
/// `min(contrast(c, background), TEXT_MIN)`, never below `MARK_MIN`: dimmer, still readable.
pub fn dim(c: Rgb, background: Rgb) -> Rgb {
    let floor = contrast(c, background).clamp(MARK_MIN, TEXT_MIN);
    ensure_contrast(mix(c, background, DIM_MIX), background, floor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_branch::find_git;
    use crate::git_status::{FileDiff, LineChange, file_diff};
    use crate::theme::every_theme;
    use crate::ui_tokens::{MARK_MIN, TEXT_MIN, contrast, ensure_contrast};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn row(kind: RowKind, old: Option<usize>, new: Option<usize>) -> DiffRow {
        DiffRow { kind, old, new }
    }
    use RowKind::*;

    const HEAD: &str = "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n";

    #[test]
    fn parse_diff_keeps_every_row_in_order() {
        let p = parse_diff(&format!("{HEAD}@@ -1,3 +1,4 @@\n a\n-b\n+B\n c\n+d\n"));
        assert_eq!(p.text, "a\nb\nB\nc\nd");
        assert_eq!(
            p.rows,
            vec![
                row(Same, Some(1), Some(1)),
                row(Removed, Some(2), None),
                row(Added, None, Some(2)),
                row(Same, Some(3), Some(3)),
                row(Added, None, Some(4)),
            ]
        );
    }

    #[test]
    fn no_newline_markers_are_skipped() {
        let p = parse_diff(&format!(
            "{HEAD}@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n\\ No newline at end of file\n"
        ));
        assert_eq!(p.text, "a\nb");
        assert_eq!(p.rows.len(), 2);
    }

    #[test]
    fn crlf_lines_lose_their_cr() {
        let p = parse_diff("@@ -0,0 +1 @@\n+a\r\n");
        assert_eq!(p.text, "a");
        assert_eq!(p.rows, vec![row(Added, None, Some(1))]);
        // A CR inside a line stays.
        assert_eq!(parse_diff("@@ -0,0 +1 @@\n+a\rb\r\n").text, "a\rb");
    }

    #[test]
    fn two_hunks_have_a_gap_between() {
        let p = parse_diff(&format!("{HEAD}@@ -1 +1 @@\n-a\n+A\n@@ -9 +9 @@\n-i\n+I\n"));
        assert_eq!(
            p.rows,
            vec![
                row(Removed, Some(1), None),
                row(Added, None, Some(1)),
                row(Gap, None, None),
                row(Removed, Some(9), None),
                row(Added, None, Some(9)),
            ]
        );
        assert_eq!(p.text, "a\nA\n\ni\nI");
    }

    #[test]
    fn a_deleted_file_is_all_removed() {
        let p = parse_diff("@@ -1,2 +0,0 @@\n-x\n-y\n");
        assert_eq!(
            p.rows,
            vec![row(Removed, Some(1), None), row(Removed, Some(2), None)]
        );
        assert_eq!(p.text, "x\ny");
    }

    #[test]
    fn a_dash_dash_line_inside_a_hunk_is_content() {
        let p = parse_diff("@@ -1,2 +1,2 @@\n--- x\n+++ y\n");
        assert_eq!(p.text, "-- x\n++ y");
        assert_eq!(parse_diff(HEAD), Parsed::default());
        assert_eq!(parse_diff(""), Parsed::default());
    }

    #[test]
    fn a_blank_context_line_without_its_space_still_counts() {
        // `diff.suppressBlankEmpty=true` drops the leading space of an empty context line.
        let p = parse_diff("@@ -1,3 +1,3 @@\n a\n\n-b\n+B\n");
        assert_eq!(
            p.rows,
            vec![
                row(Same, Some(1), Some(1)),
                row(Same, Some(2), Some(2)),
                row(Removed, Some(3), None),
                row(Added, None, Some(3)),
            ]
        );
        assert_eq!(p.text, "a\n\nb\nB");
    }

    #[test]
    fn a_line_starting_with_a_non_ascii_char_does_not_panic() {
        let p = parse_diff("@@ -1 +1 @@\n\u{e9}x\n+\u{e9}y\n");
        // The unknown-prefix line is skipped; the next is a normal row.
        assert_eq!(p.rows, vec![row(Added, None, Some(1))]);
        assert_eq!(p.text, "\u{e9}y");
    }

    #[test]
    fn all_added_numbers_every_line() {
        let p = all_added("x\ny\n");
        assert_eq!(
            p.rows,
            vec![row(Added, None, Some(1)), row(Added, None, Some(2))]
        );
        assert_eq!(p.text, "x\ny");
        assert!(all_added("").rows.is_empty());
        // No trailing newline, CRLF.
        assert_eq!(all_added("a\r\nb").text, "a\nb");
        assert_eq!(all_added("a\r\nb").rows.len(), 2);
    }

    #[test]
    fn context_is_the_line_count_plus_one() {
        assert_eq!(context_for(0), 1);
        assert_eq!(context_for(10), 11);
    }

    #[test]
    fn diff_outcome_cases() {
        assert!(matches!(
            diff_outcome(FileDiff::Unified(String::new()), None),
            Outcome::Same
        ));
        assert!(matches!(
            diff_outcome(FileDiff::Unified(HEAD.into()), None),
            Outcome::Same
        ));
        assert!(matches!(
            diff_outcome(FileDiff::AllAdded, None),
            Outcome::Same
        ));
        assert!(matches!(
            diff_outcome(FileDiff::Binary, Some("x")),
            Outcome::Binary
        ));
        match diff_outcome(FileDiff::AllAdded, Some("q\n")) {
            Outcome::Rows(p) => assert_eq!(p.rows, vec![row(Added, None, Some(1))]),
            _ => panic!("rows"),
        }
        match diff_outcome(FileDiff::Unified("@@ -1 +1 @@\n-a\n+b\n".into()), None) {
            Outcome::Rows(p) => assert_eq!(p.rows.len(), 2),
            _ => panic!("rows"),
        }
    }

    #[test]
    fn sides_rebuild_old_and_new() {
        let p = parse_diff("@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n");
        assert_eq!(
            sides(&p),
            ("a\nb\nc\n".to_string(), "a\nB\nc\n".to_string())
        );
        assert_eq!(sides(&Parsed::default()), (String::new(), String::new()));
        // A gap contributes to neither side.
        let g = parse_diff("@@ -1 +1 @@\n-a\n+A\n@@ -9 +9 @@\n x\n");
        assert_eq!(sides(&g), ("a\nx\n".to_string(), "A\nx\n".to_string()));
    }

    #[test]
    fn diff_spans_take_each_row_from_its_side() {
        let rgb = |n: u8| alacritty_terminal::vte::ansi::Rgb { r: n, g: n, b: n };
        let old: Vec<Vec<crate::highlight::Span>> =
            vec![vec![(0..1, rgb(1))], vec![(0..2, rgb(2))]];
        let new: Vec<Vec<crate::highlight::Span>> =
            vec![vec![(0..1, rgb(10))], vec![(0..3, rgb(11))]];
        let rows = [
            row(Removed, Some(2), None),
            row(Same, Some(3), Some(2)),
            row(Gap, None, None),
            row(Added, None, Some(1)),
            row(Added, None, Some(9)),
        ];
        let got = diff_spans(&rows, &old, &new);
        assert_eq!(got[0], old[1]);
        assert_eq!(got[1], new[1]);
        assert!(got[2].is_empty());
        assert_eq!(got[3], new[0]);
        assert!(got[4].is_empty());
    }

    #[test]
    fn reveal_row_and_first_change() {
        let p = parse_diff("@@ -1,5 +1,6 @@\n a\n b\n-c\n-d\n+C\n+D\n+E\n f\n");
        // rows: 0 a, 1 b, 2 -c, 3 -d, 4 +C, 5 +D, 6 +E, 7 f
        assert_eq!(first_change(&p.rows), Some(2));
        assert_eq!(reveal_row(&p.rows, (3, LineChange::Changed)), Some(2));
        assert_eq!(reveal_row(&p.rows, (4, LineChange::Changed)), Some(5));
        assert_eq!(reveal_row(&p.rows, (5, LineChange::Added)), Some(6));
        assert_eq!(reveal_row(&p.rows, (2, LineChange::DeletedAfter)), Some(2));
        assert_eq!(reveal_row(&p.rows, (0, LineChange::DeletedAfter)), Some(0));
        assert_eq!(reveal_row(&p.rows, (99, LineChange::Changed)), None);
        let same = parse_diff("@@ -1,2 +1,2 @@\n a\n b\n");
        assert_eq!(first_change(&same.rows), None);
        assert_eq!(first_change(&[]), None);
        // A pure addition's own row.
        let add = parse_diff("@@ -1 +1,2 @@\n a\n+b\n");
        assert_eq!(reveal_row(&add.rows, (2, LineChange::Added)), Some(1));
    }

    #[test]
    fn dimmed_text_stays_readable_on_every_theme() {
        for (name, theme) in every_theme() {
            let (fg, bg) = (theme.foreground, theme.background);
            let d = dim(fg, bg);
            assert!(
                contrast(d, bg) >= TEXT_MIN,
                "{name}: dimmed {}",
                contrast(d, bg)
            );
            assert!(contrast(d, bg) < contrast(fg, bg), "{name}: not dimmer");
            for i in 1..=6 {
                let c = ensure_contrast(theme.palette[i], bg, MARK_MIN);
                assert!(contrast(dim(c, bg), bg) >= 3.0, "{name}: palette {i}");
            }
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fuxx-diff-view-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git_in(git: &Path, root: &Path, args: &[&str]) {
        let ok = Command::new(git)
            .arg("-C")
            .arg(root)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn file_diff_end_to_end() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = temp("e2e");
        // No commits yet: all added.
        git_in(&git, &root, &["init", "-q"]);
        fs::write(root.join("early.txt"), "x\n").unwrap();
        assert_eq!(
            file_diff(&git, &root, Path::new("early.txt"), 3),
            Some(FileDiff::AllAdded)
        );
        let old: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        fs::write(root.join("a.txt"), &old).unwrap();
        fs::write(root.join("gone.txt"), "p\nq\n").unwrap();
        fs::write(root.join("same.txt"), "s\n").unwrap();
        fs::write(root.join("bin.dat"), b"a\0b\0").unwrap();
        fs::write(root.join("sp ace*.txt"), "1\n2\n").unwrap();
        fs::write(root.join("sp ace1.txt"), "other\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        git_in(&git, &root, &["commit", "-q", "-m", "first"]);

        let new: String = old
            .replace("line 2\n", "LINE 2\n")
            .replace("line 15\n", "line 15\nextra\n");
        fs::write(root.join("a.txt"), &new).unwrap();
        let Some(FileDiff::Unified(u)) =
            file_diff(&git, &root, Path::new("a.txt"), context_for(20))
        else {
            panic!("unified");
        };
        let parsed = parse_diff(&u);
        assert!(!parsed.rows.iter().any(|r| r.kind == Gap));
        let olds: Vec<_> = parsed.rows.iter().filter_map(|r| r.old).collect();
        let news: Vec<_> = parsed.rows.iter().filter_map(|r| r.new).collect();
        assert_eq!(olds, (1..=20).collect::<Vec<_>>());
        assert_eq!(news, (1..=21).collect::<Vec<_>>());
        assert_eq!(sides(&parsed), (old.clone(), new.clone()));
        // The same change with a small context: two hunks and a gap.
        let Some(FileDiff::Unified(u)) = file_diff(&git, &root, Path::new("a.txt"), 1) else {
            panic!("unified");
        };
        assert_eq!(
            parse_diff(&u).rows.iter().filter(|r| r.kind == Gap).count(),
            1
        );

        fs::write(root.join("untracked.txt"), "u\n").unwrap();
        assert_eq!(
            file_diff(&git, &root, Path::new("untracked.txt"), 3),
            Some(FileDiff::AllAdded)
        );
        fs::write(root.join("bin.dat"), b"a\0c\0d").unwrap();
        assert_eq!(
            file_diff(&git, &root, Path::new("bin.dat"), 3),
            Some(FileDiff::Binary)
        );
        assert_eq!(
            file_diff(&git, &root, Path::new("same.txt"), 3),
            Some(FileDiff::Unified(String::new()))
        );
        fs::remove_file(root.join("gone.txt")).unwrap();
        let Some(FileDiff::Unified(u)) = file_diff(&git, &root, Path::new("gone.txt"), 2) else {
            panic!("unified");
        };
        let rows = parse_diff(&u).rows;
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.kind == Removed));
        // A glob character in a name is literal: only that file's lines.
        fs::write(root.join("sp ace*.txt"), "1\nTWO\n").unwrap();
        fs::write(root.join("sp ace1.txt"), "changed\n").unwrap();
        let Some(FileDiff::Unified(u)) = file_diff(&git, &root, Path::new("sp ace*.txt"), 2) else {
            panic!("unified");
        };
        assert_eq!(parse_diff(&u).text, "1\n2\nTWO");
        // Non-UTF-8 text (Latin-1 \xe9) still gives a diff, with the byte replaced.
        fs::write(root.join("latin.txt"), b"caf\xe9\nkeep\n").unwrap();
        git_in(&git, &root, &["add", "latin.txt"]);
        git_in(&git, &root, &["commit", "-q", "-m", "latin"]);
        fs::write(root.join("latin.txt"), b"caf\xe9 au lait\nkeep\n").unwrap();
        let Some(FileDiff::Unified(u)) = file_diff(&git, &root, Path::new("latin.txt"), 2) else {
            panic!("unified");
        };
        assert_eq!(parse_diff(&u).rows.len(), 3);
        // A textconv driver does not change the rows.
        fs::write(root.join(".gitattributes"), "conv.txt diff=up\n").unwrap();
        fs::write(root.join("conv.txt"), "abc\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        git_in(&git, &root, &["commit", "-q", "-m", "conv"]);
        git_in(&git, &root, &["config", "diff.up.textconv", "tr a-z A-Z <"]);
        fs::write(root.join("conv.txt"), "abd\n").unwrap();
        let Some(FileDiff::Unified(u)) = file_diff(&git, &root, Path::new("conv.txt"), 2) else {
            panic!("unified");
        };
        assert_eq!(parse_diff(&u).text, "abc\nabd");
        // A plain folder.
        assert_eq!(file_diff(&git, &temp("plain"), Path::new("x"), 3), None);
    }
}
