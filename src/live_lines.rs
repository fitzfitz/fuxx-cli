//! Live changes, the pure part: compare two texts line by line, and keep a record of who changed
//! each line of an open file and when.
//!
//! Pure — the caller passes the times in — so every rule is unit-tested. Lines are compared
//! without their terminators (the file tab passes its lines with `\r` and `\n` trimmed), so a
//! CRLF file and its LF twin, or a missing final newline, never count as a change.

use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use similar::{Algorithm, DiffOp, capture_diff_slices_deadline};

/// How long a comparison may take before we give up (the diff is then only approximate).
pub const LIMIT: Duration = Duration::from_millis(50);
/// How long a changed line keeps glowing.
pub const WARM: Duration = Duration::from_secs(60);

/// Which new lines are new or changed (`blocks`, merged runs), and where each old line went
/// (`new_of_old`; `None` means it was removed or changed).
#[derive(Clone, Debug, PartialEq)]
pub struct Compared {
    pub blocks: Vec<Range<usize>>,
    pub new_of_old: Vec<Option<usize>>,
}

/// Compares `old` with `new`. Returns `None` when it ran out of time (`limit`), because then the
/// answer would only be a rough guess.
pub fn compare(old: &[&str], new: &[&str], limit: Duration) -> Option<Compared> {
    let start = Instant::now();
    let ops = capture_diff_slices_deadline(Algorithm::Myers, old, new, Some(start + limit));
    if start.elapsed() >= limit {
        return None;
    }
    let mut blocks: Vec<Range<usize>> = Vec::new();
    let mut new_of_old = vec![None; old.len()];
    for op in ops {
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for k in 0..len {
                    new_of_old[old_index + k] = Some(new_index + k);
                }
            }
            DiffOp::Insert { .. } | DiffOp::Replace { .. } => {
                let r = op.new_range();
                if r.is_empty() {
                    continue;
                }
                match blocks.last_mut() {
                    Some(last) if last.end == r.start => last.end = r.end,
                    _ => blocks.push(r),
                }
            }
            DiffOp::Delete { .. } => {}
        }
    }
    Some(Compared { blocks, new_of_old })
}

/// Who changed a line.
#[derive(Clone, Debug, PartialEq)]
pub enum Who {
    Agent(AgentMark),
    You,
}

/// An agent as shown on a line: its name, the letter drawn, and its colour if it has one.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentMark {
    pub name: Arc<str>,
    pub letter: char,
    pub rgb: Option<u32>,
}

/// Whether we know who changed the line, or only guess from what was running.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sure {
    Exact,
    Guess,
}

/// One line's mark: who, how sure, and when.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub who: Who,
    pub sure: Sure,
    pub when: SystemTime,
}

/// One optional mark per line, plus the blocks of the latest change.
#[derive(Clone, Default)]
pub struct LineRecord {
    marks: Vec<Option<Mark>>,
    last: Vec<Range<usize>>,
}

impl LineRecord {
    /// A record for a file just opened: `lines` lines, none marked.
    pub fn baseline(lines: usize) -> LineRecord {
        LineRecord {
            marks: vec![None; lines],
            last: Vec::new(),
        }
    }

    /// Carries each old mark to where its line went, then stamps the changed blocks with `mark`.
    pub fn apply(&mut self, c: &Compared, new_len: usize, mark: Mark) {
        let mut next: Vec<Option<Mark>> = vec![None; new_len];
        for (old, to) in c.new_of_old.iter().enumerate() {
            let Some(to) = to else { continue };
            let moved = self.marks.get_mut(old).and_then(Option::take);
            if let (Some(m), Some(slot)) = (moved, next.get_mut(*to)) {
                *slot = Some(m);
            }
        }
        for block in &c.blocks {
            for line in block.clone() {
                if let Some(slot) = next.get_mut(line) {
                    *slot = Some(mark.clone());
                }
            }
        }
        self.marks = next;
        self.last = c.blocks.clone();
    }

    /// Changes who and how sure for the lines of the latest change only.
    pub fn revise_last(&mut self, who: Who, sure: Sure) {
        for block in &self.last {
            for line in block.clone() {
                if let Some(Some(m)) = self.marks.get_mut(line) {
                    m.who = who.clone();
                    m.sure = sure;
                }
            }
        }
    }

    /// Drops every mark and starts again with `lines` lines.
    pub fn clear(&mut self, lines: usize) {
        *self = LineRecord::baseline(lines);
    }

    pub fn mark(&self, line: usize) -> Option<&Mark> {
        self.marks.get(line).and_then(Option::as_ref)
    }

    /// Whether any line is marked.
    pub fn any(&self) -> bool {
        self.marks.iter().any(Option::is_some)
    }

    /// The blocks of the latest change.
    pub fn last_blocks(&self) -> &[Range<usize>] {
        &self.last
    }

    /// Runs of neighbouring lines still glowing at the same level, as (lines, level).
    pub fn warm_blocks(&self, now: SystemTime) -> Vec<(Range<usize>, u8)> {
        let mut out: Vec<(Range<usize>, u8)> = Vec::new();
        for (line, m) in self.marks.iter().enumerate() {
            let level = m
                .as_ref()
                .and_then(|m| glow_level(now.duration_since(m.when).unwrap_or(Duration::ZERO)));
            let Some(level) = level else { continue };
            match out.last_mut() {
                Some((r, l)) if r.end == line && *l == level => r.end = line + 1,
                _ => out.push((line..line + 1, level)),
            }
        }
        out
    }
}

/// How brightly a line of this age glows: 3 (fresh), 2, 1, or `None` once it has cooled.
pub fn glow_level(age: Duration) -> Option<u8> {
    match age.as_secs_f64() {
        s if s < 15.0 => Some(3),
        s if s < 35.0 => Some(2),
        s if s < WARM.as_secs_f64() => Some(1),
        _ => None,
    }
}

/// "just now" for the first seconds, then "12 s ago".
pub fn age_words(age: Duration) -> String {
    let s = age.as_secs();
    if s < 5 {
        "just now".to_string()
    } else {
        format!("{s} s ago")
    }
}

/// The letter drawn for a name: its first letter or digit, uppercased; `?` if it has none.
pub fn letter_of(name: &str) -> char {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .and_then(|c| c.to_uppercase().next())
        .unwrap_or('?')
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)] // one block is the common case
mod tests {
    use super::*;

    const T0: SystemTime = SystemTime::UNIX_EPOCH;

    fn secs(n: u64) -> SystemTime {
        T0 + Duration::from_secs(n)
    }

    fn mark(who: Who, when: SystemTime) -> Mark {
        Mark {
            who,
            sure: Sure::Exact,
            when,
        }
    }

    fn agent(name: &str) -> Who {
        Who::Agent(AgentMark {
            name: name.into(),
            letter: letter_of(name),
            rgb: None,
        })
    }

    #[test]
    fn compare_marks_inserted_and_changed_lines() {
        let c = compare(&["a", "b", "c"], &["a", "B", "c", "d"], LIMIT).unwrap();
        assert_eq!(c.blocks, vec![1..2, 3..4]);
        assert_eq!(c.new_of_old, vec![Some(0), None, Some(2)]);
    }

    #[test]
    fn whole_rewrite_marks_only_the_changed_line() {
        let old: Vec<String> = (0..200).map(|i| format!("line {i}")).collect();
        let mut new = old.clone();
        new[120] = "changed".to_string();
        let o: Vec<&str> = old.iter().map(String::as_str).collect();
        let n: Vec<&str> = new.iter().map(String::as_str).collect();
        let c = compare(&o, &n, LIMIT).unwrap();
        assert_eq!(c.blocks, vec![120..121]);
    }

    #[test]
    fn terminators_alone_mark_nothing() {
        let trim = |t: &'static str| -> Vec<&'static str> {
            t.split('\n').map(|l| l.trim_end_matches('\r')).collect()
        };
        let lf = "a\nb\nc";
        let crlf = "a\r\nb\r\nc";
        assert_eq!(trim(lf), trim(crlf));
        let c = compare(&trim(lf), &trim(crlf), LIMIT).unwrap();
        assert!(c.blocks.is_empty());
        // With and without a final newline: `str::lines` drops the terminator either way.
        let with: Vec<&str> = "a\nb\n".lines().collect();
        let without: Vec<&str> = "a\nb".lines().collect();
        assert_eq!(with, without);
        assert!(compare(&with, &without, LIMIT).unwrap().blocks.is_empty());
    }

    #[test]
    fn gave_up_over_the_limit() {
        let old: Vec<String> = (0..5000).map(|i| format!("old {i}")).collect();
        let new: Vec<String> = (0..5000).map(|i| format!("new {i}")).collect();
        let o: Vec<&str> = old.iter().map(String::as_str).collect();
        let n: Vec<&str> = new.iter().map(String::as_str).collect();
        assert_eq!(compare(&o, &n, Duration::ZERO), None);
    }

    #[test]
    fn marks_follow_their_lines() {
        let mut r = LineRecord::baseline(3);
        let first = Compared {
            blocks: vec![1..2],
            new_of_old: vec![Some(0), None, Some(2)],
        };
        r.apply(&first, 3, mark(Who::You, secs(1)));
        assert!(r.mark(1).is_some());
        let second = Compared {
            blocks: vec![0..2],
            new_of_old: vec![Some(2), Some(3), Some(4)],
        };
        r.apply(&second, 5, mark(agent("claude"), secs(2)));
        assert_eq!(r.mark(3).unwrap().who, Who::You);
        assert_eq!(r.mark(0).unwrap().who, agent("claude"));
        assert_eq!(r.mark(1).unwrap().who, agent("claude"));
        assert!(r.mark(2).is_none());
        assert!(r.mark(4).is_none());
    }

    #[test]
    fn revise_last_changes_only_the_last_blocks() {
        let mut r = LineRecord::baseline(2);
        let c1 = Compared {
            blocks: vec![0..1],
            new_of_old: vec![None, Some(1)],
        };
        r.apply(&c1, 2, mark(Who::You, secs(1)));
        let c2 = Compared {
            blocks: vec![1..2],
            new_of_old: vec![Some(0), None],
        };
        r.apply(&c2, 2, mark(Who::You, secs(2)));
        assert_eq!(r.last_blocks(), &[1..2]);
        r.revise_last(agent("agy"), Sure::Guess);
        assert_eq!(r.mark(0).unwrap().who, Who::You);
        assert_eq!(r.mark(0).unwrap().sure, Sure::Exact);
        assert_eq!(r.mark(1).unwrap().who, agent("agy"));
        assert_eq!(r.mark(1).unwrap().sure, Sure::Guess);
    }

    #[test]
    fn clear_drops_every_mark() {
        let mut r = LineRecord::baseline(2);
        let c = Compared {
            blocks: vec![0..2],
            new_of_old: vec![None, None],
        };
        r.apply(&c, 2, mark(Who::You, secs(1)));
        assert!(r.any());
        r.clear(4);
        assert!(!r.any());
        assert!(r.last_blocks().is_empty());
        assert!(r.mark(3).is_none());
    }

    #[test]
    fn glow_steps() {
        let g = |ms: u64| glow_level(Duration::from_millis(ms));
        assert_eq!(g(0), Some(3));
        assert_eq!(g(14_900), Some(3));
        assert_eq!(g(15_000), Some(2));
        assert_eq!(g(34_900), Some(2));
        assert_eq!(g(35_000), Some(1));
        assert_eq!(g(59_900), Some(1));
        assert_eq!(g(60_000), None);
    }

    #[test]
    fn age_words_say_just_now_then_seconds() {
        assert_eq!(age_words(Duration::ZERO), "just now");
        assert_eq!(age_words(Duration::from_millis(4900)), "just now");
        assert_eq!(age_words(Duration::from_secs(5)), "5 s ago");
        assert_eq!(age_words(Duration::from_secs(59)), "59 s ago");
    }

    #[test]
    fn warm_blocks_group_by_level() {
        let mut r = LineRecord::baseline(2);
        let c1 = Compared {
            blocks: vec![0..1],
            new_of_old: vec![None, Some(1)],
        };
        r.apply(&c1, 2, mark(Who::You, secs(0)));
        let c2 = Compared {
            blocks: vec![1..2],
            new_of_old: vec![Some(0), None],
        };
        r.apply(&c2, 2, mark(Who::You, secs(20)));
        assert_eq!(r.warm_blocks(secs(20)), vec![(0..1, 2), (1..2, 3)]);
        assert_eq!(r.warm_blocks(secs(70)), vec![(1..2, 1)]);
    }

    #[test]
    fn letter_of_names() {
        assert_eq!(letter_of("claude"), 'C');
        assert_eq!(letter_of("agy"), 'A');
        assert_eq!(letter_of("Fitz Gerald"), 'F');
        assert_eq!(letter_of(""), '?');
    }
}
