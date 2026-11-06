//! The browser tabs' visited pages (M11b), for the address field's suggestions, and what Enter in
//! that field opens.
//!
//! The list (`browser-history.json`, next to the sessions file) exists only while "Keep history
//! and data" (`browser-keep-data`) is on: visits are recorded only then, and the same wipe that
//! deletes the saved browser data deletes the file. Pure but for `BrowserHistory`, the run's copy
//! of the list (a GPUI global), and its file.

use std::path::{Path, PathBuf};

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

use crate::browser_url::normalize_url;
use crate::settings;

/// At most this many history rows under the field (the search row comes after them).
pub const SUGGESTIONS: usize = 8;
/// The list keeps this many pages, the most recently visited.
pub const HISTORY_MAX: usize = 1000;
/// Where a search goes.
pub const SEARCH_URL: &str = "https://www.google.com/search?q=";

/// One page in the list: its address, its title when last seen, when it was last visited (Unix
/// seconds) and how many times.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Visit {
    pub url: String,
    #[serde(default)]
    pub title: String,
    pub last_visit: u64,
    pub visits: u32,
}

/// The file, under `$HOME`.
pub fn history_path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/fuxx/browser-history.json")
}

/// A visit to `url` (titled `title`) at `now`: a known page is bumped and retitled, a new one
/// added; past `HISTORY_MAX`, the least recently visited go. Blank and `about:` pages aren't
/// kept.
pub fn record(list: &mut Vec<Visit>, url: &str, title: &str, now: u64) {
    if url.is_empty() || url.starts_with("about:") {
        return;
    }
    match list.iter_mut().find(|v| v.url == url) {
        Some(v) => {
            v.visits = v.visits.saturating_add(1);
            v.last_visit = now;
            if !title.trim().is_empty() {
                v.title = title.to_string();
            }
        }
        None => list.push(Visit {
            url: url.to_string(),
            title: title.to_string(),
            last_visit: now,
            visits: 1,
        }),
    }
    if list.len() > HISTORY_MAX {
        list.sort_by_key(|v| std::cmp::Reverse(v.last_visit));
        list.truncate(HISTORY_MAX);
    }
}

/// A page's title came (after its visit was recorded): the list says it too.
pub fn retitle(list: &mut [Visit], url: &str, title: &str) -> bool {
    match list.iter_mut().find(|v| v.url == url) {
        Some(v) if !title.trim().is_empty() && v.title != title => {
            v.title = title.to_string();
            true
        }
        _ => false,
    }
}

/// How much a page's visits count `age` seconds after its last one: fully for four days, then
/// less and less (as browsers rank "frecency").
fn frecency(v: &Visit, now: u64) -> u64 {
    const DAY: u64 = 24 * 60 * 60;
    let weight = match now.saturating_sub(v.last_visit) / DAY {
        0..=3 => 100,
        4..=13 => 70,
        14..=30 => 50,
        31..=89 => 30,
        _ => 10,
    };
    u64::from(v.visits) * weight
}

/// A URL without its scheme, and its host without a leading `www.`.
fn url_parts(url: &str) -> (&str, &str) {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    (rest, host.strip_prefix("www.").unwrap_or(host))
}

/// How well `v` matches what was typed (`typed`, lowercased): 0 when its host or address starts
/// with it, 1 when its title holds it; none otherwise.
fn tier(v: &Visit, typed: &str) -> Option<u8> {
    let url = v.url.to_lowercase();
    let (rest, host) = url_parts(&url);
    if host.starts_with(typed) || rest.starts_with(typed) || url.starts_with(typed) {
        return Some(0);
    }
    v.title.to_lowercase().contains(typed).then_some(1)
}

/// The history rows under the field for `typed`: pages whose host or address starts with it,
/// then those whose title holds it, each by frecency (the more recent first on a tie); at most
/// `SUGGESTIONS`. None for blank text.
pub fn suggest<'a>(list: &'a [Visit], typed: &str, now: u64) -> Vec<&'a Visit> {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(u8, &Visit)> = list
        .iter()
        .filter_map(|v| Some((tier(v, &typed)?, v)))
        .collect();
    found.sort_by(|(ta, a), (tb, b)| {
        ta.cmp(tb)
            .then(frecency(b, now).cmp(&frecency(a, now)))
            .then(b.last_visit.cmp(&a.last_visit))
    });
    found
        .into_iter()
        .take(SUGGESTIONS)
        .map(|(_, v)| v)
        .collect()
}

/// The Google search for `text`: every byte but the unreserved ones percent-encoded.
pub fn search_url(text: &str) -> String {
    let mut url = String::from(SEARCH_URL);
    for b in text.trim().bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            url.push(b as char);
        } else {
            url.push_str(&format!("%{b:02X}"));
        }
    }
    url
}

/// The last row under the field: `Search Google for “text”`.
pub fn search_label(text: &str) -> String {
    format!("Search Google for \u{201c}{}\u{201d}", text.trim())
}

/// Whether typed text is an address rather than words to search for: one word with a dot, a
/// colon or a slash in it, or `localhost`.
fn looks_like_address(text: &str) -> bool {
    !text.contains(char::is_whitespace)
        && (text.contains(['.', ':', '/']) || text.eq_ignore_ascii_case("localhost"))
}

/// What Enter in the address field opens for `text`: an address, by the http/https rule
/// (`normalize_url`, which may refuse it: the error), else a Google search for the words.
pub fn enter_target(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("empty address".to_string());
    }
    if looks_like_address(text) {
        normalize_url(text)
    } else {
        Ok(search_url(text))
    }
}

/// The run's copy of the list, read from its file the first time it's wanted.
#[derive(Default)]
pub struct BrowserHistory {
    list: Option<Vec<Visit>>,
}

impl Global for BrowserHistory {}

impl BrowserHistory {
    /// The list (read now if it hasn't been).
    pub fn list(cx: &mut App) -> &[Visit] {
        Self::loaded(cx)
    }

    /// A visit, written to the file off the UI thread. `keep`: "Keep history and data" is on;
    /// off, nothing is recorded.
    pub fn record(url: &str, title: &str, keep: bool, cx: &mut App) {
        if !keep {
            return;
        }
        let now = now_secs();
        record(Self::loaded(cx), url, title, now);
        Self::write(cx);
    }

    /// A visited page's title came: the list says it too (only while keeping history).
    pub fn retitle(url: &str, title: &str, keep: bool, cx: &mut App) {
        if keep && retitle(Self::loaded(cx), url, title) {
            Self::write(cx);
        }
    }

    /// The list the UI screenshots suggest from (never read from or written to a file).
    #[cfg(feature = "ui-shots")]
    pub fn set_for_shots(list: Vec<Visit>, cx: &mut App) {
        cx.default_global::<BrowserHistory>().list = Some(list);
    }

    /// The wipe: the list goes, and its file.
    pub fn forget(cx: &mut App) {
        cx.default_global::<BrowserHistory>().list = Some(Vec::new());
        let Some(path) = crate::workspace::home_dir().map(|h| history_path(&h)) else {
            return;
        };
        match std::fs::remove_file(&path) {
            Ok(()) => eprintln!("[fuxx] browser: history deleted"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => eprintln!("[fuxx] browser: couldn't delete {}: {e}", path.display()),
        }
    }

    fn loaded(cx: &mut App) -> &mut Vec<Visit> {
        let history = cx.default_global::<BrowserHistory>();
        history.list.get_or_insert_with(read_file)
    }

    fn write(cx: &mut App) {
        let Some(path) = crate::workspace::home_dir().map(|h| history_path(&h)) else {
            return;
        };
        let text = match serde_json::to_string(Self::loaded(cx)) {
            Ok(text) => text,
            Err(e) => return eprintln!("[fuxx] browser: history not saved: {e}"),
        };
        cx.background_executor()
            .spawn(async move {
                if let Err(e) = settings::write_atomic(&path, &text) {
                    eprintln!("[fuxx] browser: couldn't save {}: {e}", path.display());
                }
            })
            .detach();
    }
}

/// The file's list; empty when there is none or it can't be read.
fn read_file() -> Vec<Visit> {
    let Some(path) = crate::workspace::home_dir().map(|h| history_path(&h)) else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 60 * 60;
    const NOW: u64 = 1_800_000_000;

    fn visit(url: &str, title: &str, days_ago: u64, visits: u32) -> Visit {
        Visit {
            url: url.into(),
            title: title.into(),
            last_visit: NOW - days_ago * DAY,
            visits,
        }
    }

    fn urls(found: Vec<&Visit>) -> Vec<&str> {
        found.into_iter().map(|v| v.url.as_str()).collect()
    }

    #[test]
    fn host_and_address_prefixes_come_before_titles_then_frecency() {
        let list = vec![
            visit("https://docs.rs/gpui", "gpui - Rust", 1, 3),
            visit(
                "https://github.com/zed-industries/zed",
                "Zed: docs and more",
                0,
                9,
            ),
            visit("http://localhost:3000/", "My app", 0, 20),
            visit("https://www.docker.com/", "Docker", 2, 1),
            visit("https://doc.rust-lang.org/book/", "The Rust Book", 40, 50),
        ];
        // `doc`: three hosts start with it (www. ignored), by frecency: the book was visited
        // most but long ago (50 × 30), docs.rs lately (3 × 100), docker once (1 × 100). Then
        // Zed's title holds it.
        assert_eq!(
            urls(suggest(&list, "doc", NOW)),
            [
                "https://doc.rust-lang.org/book/",
                "https://docs.rs/gpui",
                "https://www.docker.com/",
                "https://github.com/zed-industries/zed",
            ]
        );
        // The address after the host, and with its scheme.
        assert_eq!(
            urls(suggest(&list, "localhost:3", NOW)),
            ["http://localhost:3000/"]
        );
        assert_eq!(
            urls(suggest(&list, "https://docs.rs/g", NOW)),
            ["https://docs.rs/gpui"]
        );
        // Any case; a title only matches inside it.
        assert_eq!(
            urls(suggest(&list, "RUST", NOW)),
            ["https://doc.rust-lang.org/book/", "https://docs.rs/gpui"]
        );
        // Nothing typed, or nothing matching: no rows.
        assert!(suggest(&list, "  ", NOW).is_empty());
        assert!(suggest(&list, "zzz", NOW).is_empty());
    }

    #[test]
    fn at_most_eight_rows_and_a_tie_goes_to_the_more_recent() {
        let list: Vec<Visit> = (0..12)
            .map(|i| visit(&format!("https://site{i}.dev/"), "", i, 1))
            .collect();
        let found = urls(suggest(&list, "site", NOW));
        assert_eq!(found.len(), SUGGESTIONS);
        // Same visits and weight for the first four days: the more recent first.
        assert_eq!(
            &found[..4],
            [
                "https://site0.dev/",
                "https://site1.dev/",
                "https://site2.dev/",
                "https://site3.dev/"
            ]
        );
    }

    #[test]
    fn visits_are_bumped_retitled_and_capped() {
        let mut list = Vec::new();
        record(&mut list, "https://a.dev/", "", NOW - 10);
        record(&mut list, "https://a.dev/", "A", NOW);
        assert_eq!(list, [visit("https://a.dev/", "A", 0, 2)]);
        // A blank title keeps the one known; blank pages aren't kept.
        record(&mut list, "https://a.dev/", " ", NOW);
        assert_eq!(list[0].title, "A");
        record(&mut list, "about:blank", "", NOW);
        record(&mut list, "", "", NOW);
        assert_eq!(list.len(), 1);
        assert!(retitle(&mut list, "https://a.dev/", "A page"));
        assert!(!retitle(&mut list, "https://a.dev/", "A page"));
        assert!(!retitle(&mut list, "https://b.dev/", "B"));
        assert_eq!(list[0].title, "A page");
        // Past the cap, the least recently visited go.
        for i in 0..HISTORY_MAX as u64 {
            record(&mut list, &format!("https://p{i}.dev/"), "", NOW + 1 + i);
        }
        assert_eq!(list.len(), HISTORY_MAX);
        assert!(!list.iter().any(|v| v.url == "https://a.dev/"));
    }

    #[test]
    fn the_search_row_and_its_address() {
        assert_eq!(
            search_url("rust closures & café"),
            "https://www.google.com/search?q=rust%20closures%20%26%20caf%C3%A9"
        );
        assert_eq!(
            search_url(" a+b=c?/#~ "),
            "https://www.google.com/search?q=a%2Bb%3Dc%3F%2F%23~"
        );
        assert_eq!(
            search_label(" gpui canvas "),
            "Search Google for \u{201c}gpui canvas\u{201d}"
        );
    }

    #[test]
    fn enter_opens_an_address_by_the_http_rule_or_searches() {
        // Addresses: the same rule agents get.
        assert_eq!(
            enter_target("localhost:3000").unwrap(),
            "http://localhost:3000"
        );
        assert_eq!(enter_target("localhost").unwrap(), "http://localhost");
        assert_eq!(
            enter_target("docs.rs/gpui").unwrap(),
            "https://docs.rs/gpui"
        );
        // With a space, it's words.
        assert_eq!(
            enter_target("https://example.com/a b").unwrap(),
            "https://www.google.com/search?q=https%3A%2F%2Fexample.com%2Fa%20b"
        );
        assert_eq!(
            enter_target("http://127.0.0.1:8080/x").unwrap(),
            "http://127.0.0.1:8080/x"
        );
        // A scheme other than http(s) is refused, not searched for.
        assert!(enter_target("javascript:alert(1)").is_err());
        assert!(enter_target("file:///etc/passwd").is_err());
        // Words: a search.
        assert_eq!(
            enter_target("rust closures").unwrap(),
            "https://www.google.com/search?q=rust%20closures"
        );
        assert_eq!(
            enter_target("gpui").unwrap(),
            "https://www.google.com/search?q=gpui"
        );
        assert!(enter_target("   ").is_err());
    }
}
