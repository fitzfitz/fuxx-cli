//! The outline of a file's blocks (M11ah): worked out off the UI thread with each read (as
//! `live_lines::compare` is), from the lines' indentation (a Markdown file: from its headings).
//! Guides, folds, the sticky line, the breadcrumb and the Diff cards' titles all read it. Pure:
//! no GPUI, no I/O. The lines the colouring pass names (`highlight::Coloured::named`) arrive
//! later (`Outline::name`); until then, and for plain text, the innermost block is the fallback.

use std::collections::BTreeSet;
use std::ops::Range;
use std::time::{Duration, Instant};

/// Give up after this long (a huge file with no structure): that read gets no outline.
pub const LIMIT: Duration = Duration::from_millis(50);
/// A breadcrumb part is cut to this many characters, then `…`.
pub const PART_MAX: usize = 32;
/// The step when a file has no nested lines to tell it from.
const DEFAULT_STEP: usize = 4;
/// How many lines are worked out between looks at the clock.
const CLOCK_EVERY: usize = 256;

/// A run of lines: the opener, what is nested under it, and (for braces and the like) its closer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The opener's line.
    pub start: usize,
    /// One past the last line, the closer included.
    pub end: usize,
    /// The opener's indent, in cells.
    pub indent: usize,
    /// How many blocks hold this one.
    pub depth: usize,
    /// Whether the colouring pass named the opener (a function, a type, a heading...).
    pub named: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    /// The indent step in cells: the most common one between nested lines, else four.
    pub step: usize,
    /// By start line: an outer block before the ones inside it.
    pub blocks: Vec<Block>,
    /// Each block's holder (an index into `blocks`).
    parents: Vec<Option<usize>>,
    /// Each line's guide count.
    levels: Vec<usize>,
    /// The lines last given to `name` (ascending), for a reload to carry over.
    named_lines: Vec<usize>,
}

/// The indent of `line` in cells (a tab is four), or None for a blank line.
pub fn indent_cells(line: &str) -> Option<usize> {
    let mut cells = 0;
    for c in line.chars() {
        match c {
            ' ' => cells += 1,
            '\t' => cells += 4,
            c if c.is_whitespace() => {}
            _ => return Some(cells),
        }
    }
    None
}

/// Whether `line` only closes a block: `}`, `)`, `]`, `end` or a closing tag.
fn is_closer(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with(['}', ')', ']']) || t.starts_with("</") {
        return true;
    }
    t.strip_prefix("end")
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
}

/// The Markdown heading level of `line` (1 to 6), if it is a heading.
fn heading_level(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let hashes = rest.chars().take_while(|c| *c == '#').count();
    let after = &rest[hashes..];
    (1..=6).contains(&hashes).then_some(())?;
    (after.is_empty() || after.starts_with([' ', '\t'])).then_some(hashes)
}

/// The fence a Markdown line opens or closes: its character and length.
fn fence(line: &str) -> Option<(char, usize)> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = t.chars().take_while(|x| *x == c).count();
    (n >= 3 && line.len() - t.len() <= 3).then_some((c, n))
}

/// The outline of `lines` (without their endings): for Markdown the headings' blocks, else the
/// indentation's. None: gave up after `limit`.
pub fn outline(lines: &[&str], markdown: bool, limit: Duration) -> Option<Outline> {
    let started = Instant::now();
    let over = |i: usize| i.is_multiple_of(CLOCK_EVERY) && started.elapsed() >= limit;
    let indents: Vec<Option<usize>> = lines.iter().map(|l| indent_cells(l)).collect();
    let mut out = Outline {
        step: DEFAULT_STEP,
        levels: vec![0; lines.len()],
        ..Outline::default()
    };
    if markdown {
        headings(lines, &indents, &mut out, &over)?;
        return Some(out);
    }
    // The first non-blank line at or after each line, from the end.
    let mut next_indent = vec![None; lines.len() + 1];
    for i in (0..lines.len()).rev() {
        next_indent[i] = indents[i].or(next_indent[i + 1]);
    }
    // The most common step: how much deeper a line is than the non-blank one before it.
    let mut steps: Vec<usize> = Vec::new();
    let mut open: Vec<usize> = Vec::new(); // indices into out.blocks
    let mut last: Option<(usize, usize)> = None; // the last non-blank line and its indent
    for (i, line) in lines.iter().enumerate() {
        if over(i) {
            return None;
        }
        let Some(indent) = indents[i] else { continue };
        if let Some((_, before)) = last
            && indent > before
        {
            let d = indent - before;
            if steps.len() <= d {
                steps.resize(d + 1, 0);
            }
            steps[d] += 1;
        }
        // Blocks this line is no longer nested under end before it, or with it (a closer at
        // the opener's indent that opens nothing itself).
        let opens = next_indent[i + 1].is_some_and(|n| n > indent);
        let before_end = last.map_or(i, |(l, _)| l + 1);
        while let Some(&b) = open.last() {
            let block = &mut out.blocks[b];
            if block.indent < indent {
                break;
            }
            block.end = if block.indent == indent && !opens && is_closer(line) {
                i + 1
            } else {
                before_end
            };
            open.pop();
        }
        if opens {
            out.blocks.push(Block {
                start: i,
                end: lines.len(),
                indent,
                depth: open.len(),
                named: false,
            });
            out.parents.push(open.last().copied());
            open.push(out.blocks.len() - 1);
        }
        last = Some((i, indent));
    }
    let end = last.map_or(0, |(l, _)| l + 1);
    for b in open {
        out.blocks[b].end = end;
    }
    out.step = steps
        .iter()
        .enumerate()
        .skip(1)
        .rev() // the smaller step wins a tie
        .max_by_key(|(_, n)| **n)
        .filter(|(_, n)| **n > 0)
        .map_or(DEFAULT_STEP, |(d, _)| d);
    out.levels_from(&indents);
    Some(out)
}

/// Markdown: a heading's block runs to the line before the next heading of the same or a higher
/// level (a `#` inside a fenced code block is no heading), minus trailing blank lines.
fn headings(
    lines: &[&str],
    indents: &[Option<usize>],
    out: &mut Outline,
    over: &dyn Fn(usize) -> bool,
) -> Option<()> {
    let mut open: Vec<(usize, usize)> = Vec::new(); // (block index, level)
    let mut fenced: Option<(char, usize)> = None;
    let mut last = None; // the last non-blank line
    for (i, line) in lines.iter().enumerate() {
        if over(i) {
            return None;
        }
        if indents[i].is_none() {
            continue;
        }
        let level = match (fenced, fence(line)) {
            (None, Some(f)) => {
                fenced = Some(f);
                None
            }
            (Some((c, n)), Some((c2, n2))) if c == c2 && n2 >= n => {
                fenced = None;
                None
            }
            (Some(_), _) => None,
            (None, None) => heading_level(line),
        };
        if let Some(level) = level {
            let end = last.map_or(i, |l| l + 1);
            while let Some(&(b, lv)) = open.last() {
                if lv < level {
                    break;
                }
                out.blocks[b].end = end;
                open.pop();
            }
            out.blocks.push(Block {
                start: i,
                end: lines.len(),
                indent: indents[i].unwrap_or(0),
                depth: open.len(),
                named: false,
            });
            out.parents.push(open.last().map(|&(b, _)| b));
            open.push((out.blocks.len() - 1, level));
        }
        last = Some(i);
    }
    let end = last.map_or(0, |l| l + 1);
    for (b, _) in open {
        out.blocks[b].end = end;
    }
    Some(())
}

/// A breadcrumb part for an opener line: trimmed, a trailing `{`, `:` or `=>` dropped, cut to
/// `PART_MAX` characters then `…`; for Markdown without the leading `#`s.
pub fn part_words(line: &str, markdown: bool) -> String {
    let mut text = line.trim();
    if markdown {
        text = text.trim_start_matches('#').trim_start();
    }
    loop {
        let before = text;
        for tail in ["{", "=>", ":"] {
            if let Some(rest) = text.strip_suffix(tail) {
                text = rest.trim_end();
            }
        }
        if text == before {
            break;
        }
    }
    if text.chars().count() > PART_MAX {
        let cut: String = text.chars().take(PART_MAX).collect();
        format!("{cut}\u{2026}")
    } else {
        text.to_string()
    }
}

impl Outline {
    /// Each line's guide count: the whole steps in its indent. A blank line inside a block takes
    /// the block's own (the innermost one holding it).
    fn levels_from(&mut self, indents: &[Option<usize>]) {
        let step = self.step.max(1);
        for (i, indent) in indents.iter().enumerate() {
            self.levels[i] = match indent {
                Some(n) => n / step,
                None => self
                    .innermost(i)
                    .map_or(0, |b| self.blocks[b].indent / step + 1),
            };
        }
    }

    /// Whether the file has any blocks (no: no guides, folds, sticky line or breadcrumb).
    pub fn has_structure(&self) -> bool {
        !self.blocks.is_empty()
    }

    /// Marks the blocks that start on one of `named_lines` (ascending) and unmarks the others.
    /// True when a mark changed.
    pub fn name(&mut self, named_lines: &[usize]) -> bool {
        if self.named_lines != named_lines {
            self.named_lines = named_lines.to_vec();
        }
        let mut changed = false;
        for b in &mut self.blocks {
            let named = named_lines.binary_search(&b.start).is_ok();
            changed |= std::mem::replace(&mut b.named, named) != named;
        }
        changed
    }

    /// The lines last given to `name`.
    pub fn named_lines(&self) -> &[usize] {
        &self.named_lines
    }

    /// How many guides `line` has.
    pub fn levels(&self, line: usize) -> usize {
        self.levels.get(line).copied().unwrap_or(0)
    }

    /// The innermost block holding `line` (an index into `blocks`).
    fn innermost(&self, line: usize) -> Option<usize> {
        let mut at = self
            .blocks
            .partition_point(|b| b.start <= line)
            .checked_sub(1);
        while let Some(i) = at {
            if self.blocks[i].end > line {
                return Some(i);
            }
            at = self.parents[i];
        }
        None
    }

    /// The blocks holding `line` (`start <= line < end`), outer to inner.
    pub fn containing(&self, line: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut at = self.innermost(line);
        while let Some(i) = at {
            chain.push(i);
            at = self.parents[i];
        }
        chain.reverse();
        chain
    }

    /// The block starting on `line`.
    pub fn starting_at(&self, line: usize) -> Option<usize> {
        self.blocks.binary_search_by_key(&line, |b| b.start).ok()
    }

    /// The block that names where `line` is: the innermost named one holding it, else the
    /// innermost.
    pub fn titled(&self, line: usize) -> Option<usize> {
        let chain = self.containing(line);
        chain
            .iter()
            .rev()
            .find(|&&i| self.blocks[i].named)
            .or(chain.last())
            .copied()
    }

    /// The blocks that name where `line` is, outer to inner: the named ones holding it, or when
    /// none is named, all of them.
    pub fn path(&self, line: usize) -> Vec<usize> {
        let chain = self.containing(line);
        let named: Vec<usize> = chain
            .iter()
            .copied()
            .filter(|&i| self.blocks[i].named)
            .collect();
        if named.is_empty() { chain } else { named }
    }

    /// The block whose opener the sticky line shows with `top` the first line in view: among the
    /// blocks holding `top` whose opener is above it, the innermost named one, else the innermost.
    pub fn sticky(&self, top: usize) -> Option<usize> {
        let above: Vec<usize> = self
            .containing(top)
            .into_iter()
            .filter(|&i| self.blocks[i].start < top)
            .collect();
        above
            .iter()
            .rev()
            .find(|&&i| self.blocks[i].named)
            .or(above.last())
            .copied()
    }
}

/// The lines folded blocks hide (each folded block's lines after its opener, the closer
/// included), in order and merged: a fold inside a folded one adds nothing (the outer wins). A
/// start with no block there (an old outline's) hides nothing.
pub fn hidden_ranges(outline: &Outline, folded: &BTreeSet<usize>) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for &start in folded {
        let Some(b) = outline.starting_at(start) else {
            continue;
        };
        let hide = start + 1..outline.blocks[b].end;
        if hide.is_empty() {
            continue;
        }
        // In start order: one inside the last run is part of it; one touching it joins it.
        match out.last_mut() {
            Some(last) if hide.start <= last.end => last.end = last.end.max(hide.end),
            _ => out.push(hide),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(lines: &[&str]) -> Outline {
        outline(lines, false, Duration::from_secs(5)).unwrap()
    }

    fn spans(o: &Outline) -> Vec<(usize, usize)> {
        o.blocks.iter().map(|b| (b.start, b.end)).collect()
    }

    #[test]
    fn indentation_opens_a_block_when_the_next_line_is_deeper() {
        let o = make(&["fn a() {", "    x();", "}", "fn b() {}"]);
        assert_eq!(spans(&o), vec![(0, 3)]);
        assert_eq!(o.starting_at(0), Some(0));
        assert_eq!(o.starting_at(3), None);
        assert!(o.has_structure());
    }

    #[test]
    fn closers_join_their_block() {
        for closer in ["}", ")", "]", "end", "end;", "</div>"] {
            let o = make(&["open", "    body", closer, "after"]);
            assert_eq!(spans(&o), vec![(0, 3)], "{closer}");
        }
        // Not a closer: a word that only starts with `end`, or a line that opens a block itself.
        assert_eq!(spans(&make(&["a", "    b", "endless", "c"])), vec![(0, 2)]);
        let chained = make(&["if x {", "    a", "} else {", "    b", "}"]);
        assert_eq!(spans(&chained), vec![(0, 2), (2, 5)]);
        // A deeper `}` is inside the block, not its closer (and not `b`'s either: it is deeper).
        let deeper = make(&["a {", "    b", "        }", "c"]);
        assert_eq!(spans(&deeper), vec![(0, 3), (1, 3)]);
        // Only the block at the closer's own indent takes it.
        let nested = make(&["a {", "    b {", "        c", "    }", "}"]);
        assert_eq!(spans(&nested), vec![(0, 5), (1, 4)]);
        assert_eq!((nested.blocks[0].depth, nested.blocks[1].depth), (0, 1));
    }

    #[test]
    fn blank_lines_inside_a_block_belong_to_it() {
        let o = make(&["fn a() {", "    x();", "", "    y();", "}"]);
        assert_eq!(spans(&o), vec![(0, 5)]);
        assert_eq!(o.containing(2), vec![0]);
        assert_eq!(o.levels(2), 1, "the blank line takes the block's guide");
        assert_eq!((o.levels(1), o.levels(4)), (1, 0));
    }

    #[test]
    fn a_trailing_blank_line_does_not() {
        let o = make(&["fn a() {", "    x();", "", ""]);
        assert_eq!(spans(&o), vec![(0, 2)]);
        assert_eq!((o.levels(2), o.levels(3)), (0, 0));
        assert!(o.containing(2).is_empty());
    }

    #[test]
    fn markdown_headings_run_to_the_next_same_or_higher() {
        let o = outline(
            &["# A", "## B", "text", "## C", "# D"],
            true,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(spans(&o), vec![(0, 4), (1, 3), (3, 4), (4, 5)]);
        assert_eq!(
            o.blocks.iter().map(|b| b.depth).collect::<Vec<_>>(),
            [0, 1, 1, 0]
        );
        assert_eq!(o.containing(2), vec![0, 1]);
    }

    #[test]
    fn fenced_code_hashes_are_not_headings() {
        let o = outline(
            &[
                "# Setup",
                "```sh",
                "# install",
                "make",
                "```",
                "~~~",
                "# x",
                "~~~",
                "## Next",
            ],
            true,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(spans(&o), vec![(0, 9), (8, 9)]);
    }

    #[test]
    fn tabs_count_four_and_odd_indents_round_down() {
        let o = make(&["a", "  b", "    c", "      d", "   e", "      f"]);
        assert_eq!(o.step, 2);
        assert_eq!(o.levels(4), 1, "three cells in a two-cell file: one level");
        assert_eq!(o.levels(3), 3);
        assert_eq!(indent_cells("\tx"), Some(4));
        assert_eq!(indent_cells(" \t x"), Some(6));
        assert_eq!(indent_cells("   "), None);
        let tabbed = make(&["a", "\tb", "\t\tc"]);
        assert_eq!((tabbed.step, tabbed.levels(1), tabbed.levels(2)), (4, 1, 2));
    }

    #[test]
    fn the_step_is_the_most_common_one_default_four() {
        let two = make(&["a", "  b", "    c", "  d", "    e"]);
        assert_eq!(two.step, 2);
        let flat = make(&["a", "b", "c"]);
        assert_eq!(flat.step, 4);
        // A tie goes to the smaller step.
        assert_eq!(make(&["a", "  b", "c", "    d"]).step, 2);
    }

    #[test]
    fn no_structure_in_flat_text() {
        let o = make(&["one", "two", "three"]);
        assert!(!o.has_structure());
        assert_eq!(o.levels(1), 0);
        assert_eq!(o.titled(1), None);
    }

    #[test]
    fn empty_and_blank_files_have_no_blocks() {
        assert!(!make(&[]).has_structure());
        assert!(!make(&[""]).has_structure());
        assert!(!make(&["", "   ", ""]).has_structure());
        let md = outline(&[], true, Duration::from_secs(5)).unwrap();
        assert!(!md.has_structure());
    }

    #[test]
    fn gave_up_over_the_limit() {
        let lines: Vec<String> = (0..50_000).map(|i| " ".repeat(i % 200 + 1) + "x").collect();
        let slices: Vec<&str> = lines.iter().map(String::as_str).collect();
        assert!(outline(&slices, false, Duration::ZERO).is_none());
        assert!(outline(&slices, true, Duration::ZERO).is_none());
        assert!(outline(&slices, false, Duration::from_secs(30)).is_some());
    }

    fn rust() -> Outline {
        make(&[
            "impl S {",
            "    fn r() {",
            "        if x {",
            "            y();",
            "        }",
            "    }",
            "}",
        ])
    }

    #[test]
    fn names_prefer_the_innermost_named_block() {
        let mut o = rust();
        // No names: the innermost block.
        assert_eq!(o.titled(3), Some(2));
        assert_eq!(o.path(3), vec![0, 1, 2]);
        assert!(o.name(&[0, 1]));
        assert!(!o.name(&[0, 1]), "the same again changes nothing");
        assert_eq!(o.titled(3), Some(1));
        assert_eq!(o.path(3), vec![0, 1]);
        assert_eq!(o.titled(0), Some(0));
        assert_eq!(
            o.sticky(1),
            Some(0),
            "fn's opener is the top; impl's is above"
        );
        assert_eq!(o.sticky(3), Some(1));
        assert_eq!(o.sticky(0), None);
        assert_eq!(o.sticky(6), Some(0));
        // Unnamed: the innermost block whose opener is above the top.
        let plain = rust();
        assert_eq!(plain.sticky(3), Some(2));
        assert_eq!(plain.sticky(2), Some(1));
        assert!(o.name(&[0]), "a name taken back is a change");
        assert_eq!(o.titled(3), Some(0));
    }

    #[test]
    fn part_words_trim_and_cut() {
        assert_eq!(
            part_words("    pub fn refresh(&mut self) -> u8 {", false),
            "pub fn refresh(&mut self) -> u8"
        );
        assert_eq!(part_words("def refresh(self):", false), "def refresh(self)");
        assert_eq!(part_words("    Some(x) => {", false), "Some(x)");
        let long = "a".repeat(40);
        assert_eq!(
            part_words(&long, false),
            format!("{}\u{2026}", "a".repeat(32))
        );
        assert_eq!(part_words(&"b".repeat(32), false), "b".repeat(32));
        assert_eq!(part_words("## Install", true), "Install");
    }

    #[test]
    fn hidden_ranges_merge_nested_folds() {
        // 0 fn a {      block 0..6
        // 1     if x {  block 1..4
        // 2         y
        // 3     }
        // 4     z
        // 5 }
        // 6 fn b {      block 6..9
        // 7     w
        // 8 }
        let o = make(&[
            "fn a {",
            "    if x {",
            "        y",
            "    }",
            "    z",
            "}",
            "fn b {",
            "    w",
            "}",
        ]);
        assert_eq!(spans(&o), vec![(0, 6), (1, 4), (6, 9)]);
        let set = |s: &[usize]| s.iter().copied().collect::<BTreeSet<usize>>();
        assert_eq!(hidden_ranges(&o, &set(&[])), Vec::<Range<usize>>::new());
        assert_eq!(hidden_ranges(&o, &set(&[1])), vec![2..4]);
        // The outer wins: the inner fold is inside it.
        assert_eq!(hidden_ranges(&o, &set(&[0, 1])), vec![1..6]);
        assert_eq!(hidden_ranges(&o, &set(&[1, 6])), vec![2..4, 7..9]);
        // Every block folded: the two top-level ones.
        assert_eq!(hidden_ranges(&o, &set(&[0, 1, 6])), vec![1..6, 7..9]);
        // A start with no block (an old outline's) hides nothing.
        assert_eq!(
            hidden_ranges(&o, &set(&[2, 40])),
            Vec::<Range<usize>>::new()
        );
    }
}
