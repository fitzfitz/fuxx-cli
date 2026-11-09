//! The ⌘K palette's fuzzy matcher (M11j). Pure: no GPUI, no I/O.

use std::ops::Range;

pub const EXACT: i32 = 1000;
pub const PREFIX: i32 = 50;
pub const NAME_BONUS: i32 = 8;
pub const LOOSE: i32 = 1;
pub const NUMBER: i32 = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    /// Char indices into the text, sorted and unique.
    pub positions: Vec<usize>,
}

const LETTER: i32 = 16;
const WORD_START: i32 = 24;
const RUN: i32 = 16;
const GAP_CAP: i32 = 12;
const LEAD_CAP: i32 = 8;
const NEG: i32 = i32::MIN;

fn eq_ci(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

fn is_word_start(text: &[char], j: usize) -> bool {
    if j == 0 {
        return true;
    }
    let prev = text[j - 1];
    " /-_.:(".contains(prev) || (prev.is_lowercase() && text[j].is_uppercase())
}

/// Whether `word` is a subsequence of `text` (case-insensitive): the O(n) check before a word's
/// placement, so a text that can't match skips it.
fn is_subsequence(word: &[char], text: &str) -> bool {
    let mut letters = word.iter().peekable();
    for c in text.chars() {
        match letters.peek() {
            None => break,
            Some(&&w) if eq_ci(w, c) => {
                letters.next();
            }
            Some(_) => {}
        }
    }
    letters.peek().is_none()
}

/// Best placement of one word as a subsequence of `text`: DP over (word letter, text char),
/// O(word * text) with a constant-size gap window plus a running prefix maximum. `letter[j]` is
/// what a letter matched at `j` scores. Returns the score and the char positions.
fn place_word(word: &[char], text: &[char], letter: &[i32]) -> Option<(i32, Vec<usize>)> {
    let (q, n) = (word.len(), text.len());
    if q == 0 || q > n {
        return None;
    }
    // Row `i` of the table is `dp[i * n..(i + 1) * n]` (one buffer, not one per row).
    let mut dp = vec![NEG; q * n];
    let mut from = vec![usize::MAX; q * n];
    for j in 0..n {
        if eq_ci(word[0], text[j]) {
            dp[j] = letter[j] - (j as i32).min(LEAD_CAP);
        }
    }
    for i in 1..q {
        let (done, rest) = dp.split_at_mut(i * n);
        let prev_row = &done[(i - 1) * n..];
        let row = &mut rest[..n];
        let from_row = &mut from[i * n..(i + 1) * n];
        // Best (score, index) over k <= j - 14, whose gap penalty is the cap for every k.
        let mut far: (i32, usize) = (NEG, usize::MAX);
        for j in i..n {
            if j >= 14 {
                let k = j - 14;
                if prev_row[k] != NEG && prev_row[k] > far.0 {
                    far = (prev_row[k], k);
                }
            }
            if !eq_ci(word[i], text[j]) {
                continue;
            }
            let mut best = (NEG, usize::MAX);
            if far.0 != NEG {
                best = (far.0 - GAP_CAP, far.1);
            }
            for (k, &prev) in prev_row
                .iter()
                .enumerate()
                .take(j)
                .skip(j.saturating_sub(13))
            {
                if prev == NEG {
                    continue;
                }
                let adj = if k + 1 == j {
                    RUN
                } else {
                    -((j - k - 1) as i32).min(GAP_CAP)
                };
                if prev + adj > best.0 {
                    best = (prev + adj, k);
                }
            }
            if best.0 != NEG {
                row[j] = best.0 + letter[j];
                from_row[j] = best.1;
            }
        }
    }
    let (mut j, mut score) = (usize::MAX, NEG);
    for (k, &v) in dp[(q - 1) * n..].iter().enumerate() {
        if v > score {
            score = v;
            j = k;
        }
    }
    if score == NEG {
        return None;
    }
    let mut positions = vec![0; q];
    for i in (0..q).rev() {
        positions[i] = j;
        if i > 0 {
            j = from[i * n + j];
        }
    }
    Some((score, positions))
}

fn match_words(query: &str, text: &str, name_from: Option<usize>) -> Option<Match> {
    let words: Vec<Vec<char>> = query
        .split_whitespace()
        .map(|w| w.chars().collect())
        .collect();
    if words.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    // Most paths can't match: they skip the placement (and its buffers) altogether.
    if !words.iter().all(|w| is_subsequence(w, text)) {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    // What a letter matched at each char scores, the same for every word.
    let letter: Vec<i32> = (0..chars.len())
        .map(|j| {
            let bonus = match name_from {
                Some(from) if j >= from => NAME_BONUS,
                _ => 0,
            };
            LETTER
                + bonus
                + if is_word_start(&chars, j) {
                    WORD_START
                } else {
                    0
                }
        })
        .collect();
    let mut score = 0;
    let mut positions = Vec::new();
    for w in &words {
        let (s, p) = place_word(w, &chars, &letter)?;
        score += s;
        positions.extend(p);
    }
    positions.sort_unstable();
    positions.dedup();
    let trimmed: Vec<char> = query.trim().chars().collect();
    if chars.len() >= trimmed.len() && trimmed.iter().zip(&chars).all(|(a, b)| eq_ci(*a, *b)) {
        score += PREFIX;
        if chars.len() == trimmed.len() {
            score += EXACT;
        }
    }
    Some(Match { score, positions })
}

/// Fuzzy-match `query` against `text` (an item's main text). `None` when some word of the query
/// is not a subsequence. A blank query matches everything with score 0 and no highlights.
pub fn fuzzy(query: &str, text: &str) -> Option<Match> {
    match_words(query, text, None)
}

/// As `fuzzy`, for a path from the root: every matched letter after the last `/` gains
/// `NAME_BONUS`, so the file name outranks its directories.
pub fn fuzzy_path(query: &str, rel: &str) -> Option<Match> {
    let name_from = rel
        .chars()
        .enumerate()
        .filter(|(_, c)| *c == '/')
        .last()
        .map_or(0, |(i, _)| i + 1);
    match_words(query, rel, Some(name_from))
}

/// Loose match: every word of `query` is a substring (case-insensitive) of some string in `rest`.
pub fn loose(query: &str, rest: &[&str]) -> bool {
    let rest: Vec<String> = rest.iter().map(|r| r.to_lowercase()).collect();
    query
        .split_whitespace()
        .all(|w| rest.iter().any(|r| r.contains(&w.to_lowercase())))
}

/// Byte ranges of the chars at `positions` (sorted char indices); adjacent chars merge.
pub fn byte_ranges(text: &str, positions: &[usize]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut wanted = positions.iter().copied().peekable();
    for (ci, (bi, c)) in text.char_indices().enumerate() {
        let Some(&next) = wanted.peek() else { break };
        if next != ci {
            continue;
        }
        wanted.next();
        let end = bi + c.len_utf8();
        match out.last_mut() {
            Some(last) if last.end == bi => last.end = end,
            _ => out.push(bi..end),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rank<'a>(items: &[&'a str], f: impl Fn(&str) -> Option<Match>) -> Vec<&'a str> {
        let mut v: Vec<(i32, &str)> = items
            .iter()
            .filter_map(|t| f(t).map(|m| (m.score, *t)))
            .collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.0));
        v.into_iter().map(|(_, t)| t).collect()
    }

    #[test]
    fn palette_fuzzy_finds_letters_in_order() {
        assert_eq!(fuzzy("sr", "Split Right").unwrap().positions, vec![0, 6]);
        assert!(fuzzy("SPLIT", "Split Right").is_some());
        assert!(fuzzy("zz", "Split Right").is_none());
        assert!(fuzzy("rs", "Split Right").is_none());
        let blank = fuzzy("  ", "Split Right").unwrap();
        assert_eq!(blank.score, 0);
        assert!(blank.positions.is_empty());
    }

    #[test]
    fn palette_word_starts_and_runs_rank_higher() {
        let r = rank(&["Toggle Right Sidebar", "Toggle Sidebar"], |t| {
            fuzzy("tsb", t)
        });
        assert_eq!(r, vec!["Toggle Sidebar", "Toggle Right Sidebar"]);
        assert!(
            fuzzy("split", "Split Right").unwrap().score
                > fuzzy("split", "Show Splits").unwrap().score
        );
    }

    #[test]
    fn palette_exact_and_prefix_bonuses() {
        let d = fuzzy("paste", "Paste").unwrap().score - fuzzy("paste", "Paste All").unwrap().score;
        assert!(d >= EXACT - 10, "diff {d}");
        assert!(
            fuzzy("new", "New Tab").unwrap().score
                > fuzzy("new", "Renew").unwrap().score + PREFIX - 1
        );
    }

    #[test]
    fn palette_words_match_independently() {
        let a = fuzzy("left pane", "Focus Pane Left").unwrap();
        let b = fuzzy("pane left", "Focus Pane Left").unwrap();
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.positions, vec![6, 7, 8, 9, 11, 12, 13, 14]);
    }

    #[test]
    fn palette_paths_prefer_the_file_name() {
        let r = rank(
            &["src/domain/mod.rs", "docs/maintenance.md", "src/main.rs"],
            |t| fuzzy_path("main", t),
        );
        assert_eq!(
            r,
            vec!["src/main.rs", "docs/maintenance.md", "src/domain/mod.rs"]
        );
        assert!(fuzzy_path("ü", "new dir/ü.txt").is_some());
        assert_eq!(fuzzy_path("ü", "new dir/ü.txt").unwrap().positions, vec![8]);
    }

    #[test]
    fn palette_loose_matches_every_word_somewhere() {
        assert!(loose("api auth", &["api", "edit auth.rs?"]));
        assert!(!loose("web", &["api"]));
    }

    #[test]
    fn palette_byte_ranges_follow_chars() {
        assert_eq!(byte_ranges("Split Right", &[0, 1, 6]), vec![0..2, 6..7]);
        assert_eq!(byte_ranges("ü.txt", &[0]), vec![0..2]);
        assert_eq!(byte_ranges("aüb", &[1, 2]), vec![1..4]);
    }

    #[test]
    fn palette_fuzzy_takes_the_best_placement() {
        // "a_ab": a@0 (start, 40), a@2 (after `_`: 40, gap -1), b@3 (16, run +16) = 111. The
        // only placement: b must be 3, and the one `a` between 0 and 3 is 2.
        let m = fuzzy("aab", "a_ab").unwrap();
        assert_eq!(m.positions, vec![0, 2, 3]);
        assert_eq!(m.score, 40 + (40 - 1) + (16 + 16));
        // "xa_ab": the leftmost a@1 (16, lead -1) then b@4 (gap -2) is 29; a@3 (start, 40, lead
        // -3) then b@4 (run) is 69, so the later `a` wins.
        let m = fuzzy("ab", "xa_ab").unwrap();
        assert_eq!(m.positions, vec![3, 4]);
        assert_eq!(m.score, (40 - 3) + (16 + 16));
    }

    /// The O(n) check run before each word's placement: a case-insensitive subsequence.
    #[test]
    fn palette_subsequence_precheck() {
        let chars = |s: &str| s.chars().collect::<Vec<char>>();
        assert!(is_subsequence(&chars("sr"), "Split Right"));
        assert!(is_subsequence(&chars("SPLIT"), "Split Right"));
        assert!(!is_subsequence(&chars("rs"), "Split Right"));
        assert!(!is_subsequence(&chars("abc"), "ab"));
        assert!(is_subsequence(&chars("ü"), "new dir/Ü.txt"));
        assert!(is_subsequence(&[], "x"));
    }

    #[test]
    fn palette_long_gaps_cap_and_still_match() {
        let text = format!("a{}b", "x".repeat(40));
        let far = fuzzy("ab", &text).unwrap();
        assert_eq!(far.positions, vec![0, 41]);
        // letters 16+24, 16 (b, not a word start) - gap capped at 12
        assert_eq!(far.score, 40 + 16 - 12);
        assert_eq!(
            fuzzy("ab", &format!("a{}b", "x".repeat(5))).unwrap().score,
            40 + 16 - 5
        );
    }
}
