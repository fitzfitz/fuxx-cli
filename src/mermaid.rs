//! Mermaid diagrams in the Markdown Preview (M11ac Part 2): the pure parts.
//!
//! A fenced code block whose info string's first word is `mermaid` (any case) is drawn as a
//! diagram: rendered offline by the bundled mermaid.js in one hidden WebKit view
//! (`mermaid_render`), snapshotted to a PNG and shown as an image. Until it is ready (or if
//! it fails, or the file has too many diagrams, or this one is too large) the block stays
//! code, with one dim line under it saying why.
//!
//! Here: which blocks are diagrams and which may render (`plan`), the cache key (SHA-256 of
//! the source, light or dark) and the cache (`Lru`), the page the web view loads
//! (`page_html`) and the script each diagram runs (`render_script`), reading its answer
//! (`read_answer`), the snapshot's density (`density`), and the error line (`note`).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::agent_markdown::Md;

/// The bundled mermaid.js (`assets/vendor/mermaid/`, see its README for the version).
pub const MERMAID_JS: &str = include_str!("../assets/vendor/mermaid/mermaid.min.js");

/// At most this many diagrams are rendered per file; the rest stay code.
pub const MAX_DIAGRAMS: usize = 20;
/// A diagram's source over this many bytes stays code.
pub const MAX_SOURCE: usize = 50 * 1024;
/// The cache holds this many rendered diagrams (or errors), least recently used out first.
pub const CACHE_SIZE: usize = 64;
/// The densest snapshot (2 pixels per point, Retina's), and the most pixels a side or a whole
/// snapshot may have (a huge diagram is snapshotted less densely instead).
pub const MAX_DENSITY: f64 = 2.0;
pub const MAX_SIDE_PX: f64 = 8192.0;
pub const MAX_AREA_PX: f64 = 4096.0 * 4096.0;

/// What the line under a code block says when it isn't drawn as a diagram.
pub const TOO_MANY: &str = "more than 20 diagrams \u{2014} showing code";
pub const TOO_LARGE: &str = "too large";
/// The longest error kept for the line under the code (characters).
const ERROR_MAX: usize = 100;

/// Whether a code block's info string (`lang`, as the parser keeps it, trimmed) makes it a
/// Mermaid diagram: its first word is `mermaid`, in any case (`mermaid {init: …}` too).
pub fn is_mermaid(lang: Option<&str>) -> bool {
    lang.and_then(|l| l.split(|c: char| c.is_whitespace() || c == '{').next())
        .is_some_and(|word| word.eq_ignore_ascii_case("mermaid"))
}

/// A rendered diagram's cache key: the SHA-256 of its source, and whether it was drawn for a
/// dark theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub sha: [u8; 32],
    pub dark: bool,
}

/// The key for `source` drawn light or dark.
pub fn key(source: &str, dark: bool) -> Key {
    Key {
        sha: sha256(source.as_bytes()),
        dark,
    }
}

/// SHA-256, from macOS's CommonCrypto (in libSystem: no crate).
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    // CommonCrypto's `CC_SHA256_CTX`: 26 32-bit words.
    #[repr(C)]
    struct Ctx([u32; 26]);
    unsafe extern "C" {
        fn CC_SHA256_Init(c: *mut Ctx) -> i32;
        fn CC_SHA256_Update(c: *mut Ctx, data: *const std::ffi::c_void, len: u32) -> i32;
        fn CC_SHA256_Final(md: *mut u8, c: *mut Ctx) -> i32;
    }
    let mut ctx = Ctx([0; 26]);
    let mut out = [0u8; 32];
    // SAFETY: CommonCrypto's documented calls on a context of its size; each chunk is in
    // bounds, and `out` holds the 32 bytes `Final` writes.
    unsafe {
        CC_SHA256_Init(&mut ctx);
        for chunk in bytes.chunks(1 << 30) {
            CC_SHA256_Update(&mut ctx, chunk.as_ptr().cast(), chunk.len() as u32);
        }
        CC_SHA256_Final(out.as_mut_ptr(), &mut ctx);
    }
    out
}

/// What the Preview does with a Mermaid block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Render it (or show it from the cache).
    Diagram(Key),
    /// Past the first `MAX_DIAGRAMS` in the file: code, with `TOO_MANY`.
    TooMany,
    /// Over `MAX_SOURCE`: code, with `TOO_LARGE`.
    TooLarge,
}

/// For each of `blocks` (the file's top-level blocks), what the Preview does with it: `None`
/// for anything but a Mermaid block. The first `MAX_DIAGRAMS` Mermaid blocks (in order, a too
/// large one counted too) may render.
pub fn plan(blocks: &[Md], dark: bool) -> Vec<Option<Plan>> {
    let mut seen = 0;
    blocks
        .iter()
        .map(|md| {
            let Md::Code { lang, text } = md else {
                return None;
            };
            if !is_mermaid(lang.as_deref()) {
                return None;
            }
            seen += 1;
            Some(if seen > MAX_DIAGRAMS {
                Plan::TooMany
            } else if text.len() > MAX_SOURCE {
                Plan::TooLarge
            } else {
                Plan::Diagram(key(text, dark))
            })
        })
        .collect()
}

/// The line under a code block that isn't a diagram: `Mermaid: <why>`.
pub fn note(why: &str) -> String {
    format!("Mermaid: {why}")
}

/// An error from mermaid (or the renderer) made short for `note`: its first non-empty line,
/// trimmed (a parse error's with what it found instead: `Parse error on line 3: got 'EOF'`),
/// at most `ERROR_MAX` characters (with `…`).
pub fn short_error(raw: &str) -> String {
    let Some(first) = raw.lines().map(str::trim).find(|l| !l.is_empty()) else {
        return "unknown error".to_string();
    };
    // A parse error: what was found instead, from its "Expecting …, got 'X'" line.
    let found = raw
        .lines()
        .filter_map(|l| l.rsplit_once(", got "))
        .map(|(_, got)| got.trim())
        .next_back()
        .filter(|_| first.ends_with(':'));
    let line = match found {
        Some(got) => format!("{first} got {got}"),
        None => first.to_string(),
    };
    let line = line.as_str();
    if line.chars().count() <= ERROR_MAX {
        return line.to_string();
    }
    let mut short: String = line.chars().take(ERROR_MAX - 1).collect();
    short.push('…');
    short
}

/// A small least-recently-used map: `get` and `put` make an entry the newest; `put` past
/// `cap` entries, or past `budget` in all (each entry has a weight: a decoded image's bytes),
/// drops (and returns) the oldest. The newest entry always stays, however heavy.
pub struct Lru<K, V> {
    cap: usize,
    budget: usize,
    /// The weights' sum.
    weight: usize,
    /// Oldest first, with their weights.
    entries: VecDeque<(K, V, usize)>,
}

impl<K: PartialEq, V> Lru<K, V> {
    #[cfg(test)]
    pub fn new(cap: usize) -> Self {
        Self::with_budget(cap, usize::MAX)
    }

    pub fn with_budget(cap: usize, budget: usize) -> Self {
        Lru {
            cap,
            budget,
            weight: 0,
            entries: VecDeque::new(),
        }
    }

    #[cfg(any(test, feature = "ui-shots"))]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The weights' sum.
    #[cfg(any(test, feature = "ui-shots"))]
    pub fn weight(&self) -> usize {
        self.weight
    }

    /// The value for `key`, now the newest.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let at = self.entries.iter().position(|(k, ..)| k == key)?;
        let entry = self.entries.remove(at)?;
        self.entries.push_back(entry);
        self.entries.back().map(|(_, v, _)| v)
    }

    /// `put_weighted` with no weight.
    pub fn put(&mut self, key: K, value: V) -> Vec<(K, V)> {
        self.put_weighted(key, value, 0)
    }

    /// Keep `value` (weighing `weight`) for `key`, replacing any, as the newest; the entries
    /// dropped to stay within `cap` and `budget`, oldest first.
    pub fn put_weighted(&mut self, key: K, value: V, weight: usize) -> Vec<(K, V)> {
        if let Some(at) = self.entries.iter().position(|(k, ..)| *k == key)
            && let Some((_, _, old)) = self.entries.remove(at)
        {
            self.weight -= old;
        }
        self.entries.push_back((key, value, weight));
        self.weight += weight;
        let mut dropped = Vec::new();
        while self.entries.len() > 1 && (self.entries.len() > self.cap || self.weight > self.budget)
        {
            if let Some((k, v, w)) = self.entries.pop_front() {
                self.weight -= w;
                dropped.push((k, v));
            }
        }
        dropped
    }
}

/// A diagram's decoded image may use this many bytes of the cache's budget in all (256 MB):
/// past it the oldest go, as past `CACHE_SIZE`.
pub const CACHE_BYTES: usize = 256 * 1024 * 1024;
/// A diagram dropped from the cache isn't drawn again for this long if it's asked for at once
/// (more on screen than the cache holds): it shows as code with `TOO_MUCH` instead.
pub const REDRAW_PAUSE: Duration = Duration::from_secs(10);
pub const TOO_MUCH: &str = "too many large diagrams on screen \u{2014} showing code";

/// How a render ended.
pub enum Outcome<V> {
    /// Drawn: the image, and its decoded size in bytes.
    Drawn(V, usize),
    /// Mermaid couldn't draw it (kept: it won't draw next time either).
    Mermaid(String),
    /// WebKit failed (a timeout, the page's process gone, the page not loading): not kept as
    /// a diagram's answer; the next time it's asked for it renders once more.
    WebKit(String),
}

/// What the cache answers when a Preview asks for a diagram.
#[derive(Clone, Debug, PartialEq)]
pub enum Asked<V> {
    Ready(V),
    Failed(String),
    /// Queued (or already): the code shows meanwhile.
    Waiting,
    /// Dropped from the cache just now for room: code, with `TOO_MUCH`.
    TooMuch,
}

/// The diagrams' bookkeeping, apart from WebKit and GPUI (`mermaid_render` holds one): the
/// cache (`CACHE_SIZE` entries, `CACHE_BYTES`), the queue (each key once), who waits for each
/// (once per asker, `who`; `W` is what to tell them), WebKit failures waiting for their one
/// retry, and when diagrams were dropped for room.
pub struct Store<V, W> {
    cache: Lru<Key, Result<V, String>>,
    waiting: Vec<(Key, Vec<(u64, W)>)>,
    queue: VecDeque<(Key, String)>,
    /// WebKit failed once: rendered again the next time it's asked for.
    soft: Vec<Key>,
    /// Being rendered again after a WebKit failure: a second one is kept.
    retrying: Vec<Key>,
    dropped: VecDeque<(Key, Instant)>,
}

impl<V: Clone, W> Store<V, W> {
    pub fn new(cap: usize, budget: usize) -> Self {
        Store {
            cache: Lru::with_budget(cap, budget),
            waiting: Vec::new(),
            queue: VecDeque::new(),
            soft: Vec::new(),
            retrying: Vec::new(),
            dropped: VecDeque::new(),
        }
    }

    /// `who` asks for `key` (drawn from `source`) at `now`; `tell` is how to tell them when
    /// it's ready (kept once per `who`).
    pub fn ask(&mut self, key: Key, source: &str, who: u64, tell: W, now: Instant) -> Asked<V> {
        if let Some(done) = self.cache.get(&key) {
            return match done {
                Ok(v) => Asked::Ready(v.clone()),
                Err(e) => Asked::Failed(e.clone()),
            };
        }
        while self
            .dropped
            .front()
            .is_some_and(|(_, at)| now.saturating_duration_since(*at) >= REDRAW_PAUSE)
        {
            self.dropped.pop_front();
        }
        if self.dropped.iter().any(|(k, _)| *k == key) {
            return Asked::TooMuch;
        }
        if let Some(at) = self.soft.iter().position(|k| *k == key) {
            self.soft.remove(at);
            self.retrying.push(key);
        }
        match self.waiting.iter_mut().find(|(k, _)| *k == key) {
            Some((_, waiters)) if waiters.iter().any(|(w, _)| *w == who) => {}
            Some((_, waiters)) => waiters.push((who, tell)),
            None => {
                self.waiting.push((key, vec![(who, tell)]));
                self.queue.push_back((key, source.to_string()));
            }
        }
        Asked::Waiting
    }

    /// The next job someone still wants: each waiter kept only if `wanted` says so; a job
    /// nobody wants any more is dropped with its waiters.
    pub fn next_job(&mut self, mut wanted: impl FnMut(&Key, &W) -> bool) -> Option<(Key, String)> {
        while let Some((key, source)) = self.queue.pop_front() {
            if let Some(at) = self.waiting.iter().position(|(k, _)| *k == key) {
                self.waiting[at].1.retain(|(_, tell)| wanted(&key, tell));
                if !self.waiting[at].1.is_empty() {
                    return Some((key, source));
                }
                self.waiting.remove(at);
            }
            self.retrying.retain(|k| *k != key);
        }
        None
    }

    /// `key`'s render ended at `now`: whom to tell, and the images dropped from the cache for
    /// room (to free).
    pub fn finish(&mut self, key: Key, outcome: Outcome<V>, now: Instant) -> (Vec<W>, Vec<V>) {
        let tell = match self.waiting.iter().position(|(k, _)| *k == key) {
            Some(at) => self
                .waiting
                .remove(at)
                .1
                .into_iter()
                .map(|(_, t)| t)
                .collect(),
            None => Vec::new(),
        };
        let retried = match self.retrying.iter().position(|k| *k == key) {
            Some(at) => {
                self.retrying.remove(at);
                true
            }
            None => false,
        };
        let dropped = match outcome {
            Outcome::Drawn(v, bytes) => self.cache.put_weighted(key, Ok(v), bytes),
            Outcome::Mermaid(e) => self.cache.put(key, Err(e)),
            Outcome::WebKit(e) if retried => self.cache.put(key, Err(e)),
            Outcome::WebKit(_) => {
                // Bounded: a key nobody asks for again is forgotten after a while.
                if self.soft.len() >= CACHE_SIZE {
                    self.soft.remove(0);
                }
                self.soft.push(key);
                Vec::new()
            }
        };
        let mut freed = Vec::new();
        for (k, done) in dropped {
            if self.dropped.len() >= CACHE_SIZE {
                self.dropped.pop_front();
            }
            self.dropped.push_back((k, now));
            if let Ok(v) = done {
                freed.push(v);
            }
        }
        (tell, freed)
    }

    /// Diagrams (and errors) cached, their images' bytes, and jobs queued.
    #[cfg(any(test, feature = "ui-shots"))]
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.cache.len(), self.cache.weight(), self.queue.len())
    }
}

/// Mermaid's theme for fuxx's light or dark theme.
pub fn mermaid_theme(dark: bool) -> &'static str {
    if dark { "dark" } else { "default" }
}

/// The page behind a diagram's snapshot: its background (Mermaid's own for the theme).
pub fn page_background(dark: bool) -> &'static str {
    // Mermaid's `dark` theme is drawn for `#333` (its `background`); `default` for white.
    if dark { "#333333" } else { "#ffffff" }
}

/// The script WebKit injects into the page (a `WKUserScript`, which the page's policy doesn't
/// govern): the bundled mermaid.js, and Mermaid set up to draw only when asked
/// (`startOnLoad: false`), in `strict` security, in the theme for `dark`.
pub fn user_script(dark: bool) -> String {
    format!("{MERMAID_JS}\n;{}", init_js(dark))
}

/// Whether the web view may go where a navigation asks: only its own page, loaded from a
/// string (`about:blank`), in its main frame. Everything else (links, redirects, frames, a
/// script setting `location`) is cancelled.
pub fn navigation_allowed(main_frame: bool, url: &str) -> bool {
    main_frame && url == "about:blank"
}

/// The page the hidden web view loads (with no base URL): a Content-Security-Policy that
/// allows no request and no script of the page's own (only its inline styles, and `data:`
/// images and fonts), its colours, and the box diagrams are drawn in.
pub fn page_html(dark: bool) -> String {
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="{CSP}">
<style>
html, body {{ margin: 0; padding: 0; background: {bg}; }}
#out {{ position: absolute; left: 0; top: 0; display: inline-block; padding: 16px; background: {bg}; }}
</style>
</head><body><div id="out"></div></body></html>"#,
        bg = page_background(dark),
    )
}

/// The page's policy: nothing may be fetched (no `connect-src`, no remote script, style,
/// image, font, frame or media), and no script of the page's own runs (not even an inline
/// event handler in a diagram's label); only inline styles, and `data:` images and fonts,
/// which Mermaid makes itself. mermaid.js comes in as a user script (`user_script`), and
/// each diagram through `callAsyncJavaScript`: the page's policy doesn't govern either.
const CSP: &str = "default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; \
                   img-src data:; font-src data:";

/// Mermaid set up for the theme for `dark`: drawn only when asked, in strict security (HTML
/// tags in labels encoded, click handlers off; a `click … href` link survives, sanitised, and
/// is inert in a snapshot).
fn init_js(dark: bool) -> String {
    format!(
        "mermaid.initialize({{ startOnLoad: false, securityLevel: 'strict', theme: '{}' }});",
        mermaid_theme(dark)
    )
}

/// The body of the async function that draws `source` (`callAsyncJavaScript`): Mermaid set
/// to the theme for `dark`, the diagram rendered into `#out` at its natural size, and
/// `{"w", "h"}` (its size in CSS pixels, the padding round it included) answered as JSON;
/// or `{"error"}` with Mermaid's message.
pub fn render_script(source: &str, dark: bool) -> String {
    // serde_json escapes quotes, backslashes and control characters, and the source is a
    // string in the script, never code. (U+2028 and U+2029 are valid in a JS string.)
    let source = serde_json::to_string(source).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"const out = document.getElementById('out');
out.innerHTML = '';
document.body.style.background = '{bg}';
out.style.background = '{bg}';
try {{
  {init}
  window.__fuxxN = (window.__fuxxN || 0) + 1;
  const id = 'fuxx-mermaid-' + window.__fuxxN;
  const {{ svg }} = await mermaid.render(id, {source});
  out.innerHTML = svg;
  const el = out.querySelector('svg');
  if (!el) {{ return JSON.stringify({{ error: 'no diagram was drawn' }}); }}
  const vb = el.viewBox && el.viewBox.baseVal;
  const box = el.getBoundingClientRect();
  const w = vb && vb.width > 0 ? vb.width : box.width;
  const h = vb && vb.height > 0 ? vb.height : box.height;
  el.setAttribute('width', w);
  el.setAttribute('height', h);
  el.style.maxWidth = 'none';
  const r = out.getBoundingClientRect();
  return JSON.stringify({{ w: Math.ceil(r.width), h: Math.ceil(r.height) }});
}} catch (e) {{
  out.innerHTML = '';
  document.querySelectorAll('[id^="dfuxx-mermaid-"], [id^="fuxx-mermaid-"]').forEach(n => n.remove());
  return JSON.stringify({{ error: String((e && e.message) || e) }});
}}"#,
        bg = page_background(dark),
        init = init_js(dark),
    )
}

/// A diagram's size in CSS pixels (points), from `render_script`'s answer; or its error.
pub fn read_answer(text: &str) -> Result<(f64, f64), String> {
    let answer: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "the renderer's answer wasn't readable")?;
    if let Some(error) = answer.get("error") {
        return Err(error
            .as_str()
            .map_or_else(|| error.to_string(), str::to_string));
    }
    let side = |k: &str| answer.get(k).and_then(serde_json::Value::as_f64);
    match (side("w"), side("h")) {
        (Some(w), Some(h)) if w >= 1.0 && h >= 1.0 && w.is_finite() && h.is_finite() => Ok((w, h)),
        _ => Err("the diagram came out empty".to_string()),
    }
}

/// How many pixels per point a `w` × `h` diagram is snapshotted at: `MAX_DENSITY`, less for
/// a diagram so large a side would pass `MAX_SIDE_PX` or the whole `MAX_AREA_PX`.
pub fn density(w: f64, h: f64) -> f64 {
    let (w, h) = (w.max(1.0), h.max(1.0));
    MAX_DENSITY
        .min(MAX_SIDE_PX / w)
        .min(MAX_SIDE_PX / h)
        .min((MAX_AREA_PX / (w * h)).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(lang: Option<&str>, text: &str) -> Md {
        Md::Code {
            lang: lang.map(str::to_string),
            text: text.to_string(),
        }
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn a_mermaid_block_is_one_whose_first_word_is_mermaid_in_any_case() {
        for yes in [
            "mermaid",
            "Mermaid",
            "MERMAID",
            "mermaid {init: {}}",
            "mermaid{init}",
            "mermaid\tx",
        ] {
            assert!(is_mermaid(Some(yes)), "{yes:?}");
        }
        for no in [
            "",
            "rust",
            "mermaidx",
            "mer maid",
            "text mermaid",
            "{mermaid}",
        ] {
            assert!(!is_mermaid(Some(no)), "{no:?}");
        }
        assert!(!is_mermaid(None));
    }

    #[test]
    fn the_parser_keeps_an_indented_info_string_mermaid() {
        // ` mermaid {...}` after the fence: the parser trims it.
        let blocks = crate::agent_markdown::parse("``` mermaid {theme: x}\ngraph TD\n```\n");
        let Some(Md::Code { lang, .. }) = blocks.first() else {
            panic!("{blocks:?}");
        };
        assert!(is_mermaid(lang.as_deref()));
    }

    #[test]
    fn sha256_is_sha256() {
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn the_key_is_the_source_s_sha_and_light_or_dark() {
        assert_eq!(key("graph TD", true), key("graph TD", true));
        assert_ne!(key("graph TD", true), key("graph TD", false));
        assert_ne!(key("graph TD", true), key("graph LR", true));
        assert_eq!(key("abc", false).sha, sha256(b"abc"));
    }

    /// The vendored file is the one its README names.
    #[test]
    fn the_bundled_mermaid_is_the_pinned_release() {
        assert_eq!(
            hex(&sha256(MERMAID_JS.as_bytes())),
            "a8a6a6f805b074a8261a2825c86f99c8054acb3b35f50bb7e24a4f9f2c3c3b9f"
        );
    }

    #[test]
    fn the_plan_renders_the_first_twenty_and_keeps_large_ones_as_code() {
        let mut blocks = vec![
            Md::Para(vec![]),
            code(Some("rust"), "fn main() {}"),
            code(Some("mermaid"), "graph TD; A-->B"),
            code(Some("Mermaid"), &"x".repeat(MAX_SOURCE + 1)),
            code(Some("mermaid"), &"y".repeat(MAX_SOURCE)),
        ];
        for i in 0..20 {
            blocks.push(code(Some("mermaid"), &format!("graph TD; A-->B{i}")));
        }
        let got = plan(&blocks, true);
        assert_eq!(got.len(), blocks.len());
        assert_eq!(got[0], None);
        assert_eq!(got[1], None);
        assert_eq!(got[2], Some(Plan::Diagram(key("graph TD; A-->B", true))));
        assert_eq!(got[3], Some(Plan::TooLarge));
        assert_eq!(
            got[4],
            Some(Plan::Diagram(key(&"y".repeat(MAX_SOURCE), true)))
        );
        // Mermaid blocks 1–3 are the first three; 17 more render, then the rest are code.
        let diagrams = got
            .iter()
            .filter(|p| matches!(p, Some(Plan::Diagram(_))))
            .count();
        assert_eq!(diagrams, MAX_DIAGRAMS - 1, "20 counted, one too large");
        assert_eq!(
            got[4 + 17],
            Some(Plan::Diagram(key("graph TD; A-->B16", true)))
        );
        assert_eq!(got[4 + 18], Some(Plan::TooMany));
        assert_eq!(got[4 + 20], Some(Plan::TooMany));
    }

    #[test]
    fn the_plan_of_light_and_dark_differ_only_in_the_key() {
        let blocks = vec![code(Some("mermaid"), "graph TD")];
        assert_eq!(
            plan(&blocks, false),
            vec![Some(Plan::Diagram(key("graph TD", false)))]
        );
    }

    #[test]
    fn the_line_under_the_code_says_mermaid_and_why() {
        assert_eq!(note(TOO_LARGE), "Mermaid: too large");
        assert_eq!(
            note(TOO_MUCH),
            "Mermaid: too many large diagrams on screen \u{2014} showing code"
        );
        assert_eq!(
            note(TOO_MANY),
            "Mermaid: more than 20 diagrams \u{2014} showing code"
        );
    }

    #[test]
    fn an_error_is_its_first_line_trimmed_and_short() {
        assert_eq!(
            short_error("\n  Lexical error on line 2. Unrecognized text.\n...x\n--^"),
            "Lexical error on line 2. Unrecognized text."
        );
        assert_eq!(short_error("  "), "unknown error");
        let long = "e".repeat(300);
        let got = short_error(&long);
        assert_eq!(got.chars().count(), ERROR_MAX);
        assert!(got.ends_with('…'));
        // Characters, not bytes.
        let wide = "é".repeat(150);
        assert_eq!(short_error(&wide).chars().count(), ERROR_MAX);
    }

    /// Mermaid's parse error: its first line and what it found instead (the expected tokens,
    /// a long list, and the drawing of the line are left out).
    #[test]
    fn a_parse_error_says_its_line_and_what_was_found() {
        let raw = "Parse error on line 3:\n...lowchart TD  A -->\n---------------------^\n\
                   Expecting 'AMP', 'COLON', 'PIPE', got 'EOF'";
        assert_eq!(short_error(raw), "Parse error on line 3: got 'EOF'");
        assert_eq!(
            short_error("Parse error on line 1:\nx\n^\nExpecting 'A', got 'NEWLINE'"),
            "Parse error on line 1: got 'NEWLINE'"
        );
        // No "got": the first line as it is.
        assert_eq!(
            short_error("Parse error on line 1:\nx"),
            "Parse error on line 1:"
        );
    }

    #[test]
    fn the_cache_drops_the_least_recently_used() {
        let mut lru = Lru::new(3);
        assert!(lru.put(1, "a").is_empty());
        assert!(lru.put(2, "b").is_empty());
        assert!(lru.put(3, "c").is_empty());
        // 1 used: 2 is now the oldest.
        assert_eq!(lru.get(&1), Some(&"a"));
        assert_eq!(lru.put(4, "d"), vec![(2, "b")]);
        assert_eq!(lru.get(&2), None);
        assert_eq!(lru.len(), 3);
        // Replacing keeps the size and makes it the newest.
        assert!(lru.put(3, "C").is_empty());
        assert_eq!(lru.put(5, "e"), vec![(1, "a")]);
        assert_eq!(lru.get(&3), Some(&"C"));
        assert_eq!(lru.len(), 3);
    }

    #[test]
    fn the_cache_holds_sixty_four() {
        let mut lru = Lru::new(CACHE_SIZE);
        for i in 0..CACHE_SIZE {
            assert!(lru.put(i, i).is_empty());
        }
        assert_eq!(lru.put(CACHE_SIZE, 0), vec![(0, 0)]);
        assert_eq!(lru.len(), 64);
    }

    /// Review fix: the page runs no script of its own (`script-src 'none'`: an inline event
    /// handler Mermaid's sanitiser missed can't run either); mermaid.js comes in as a user
    /// script, which the page's policy doesn't govern.
    #[test]
    fn the_page_allows_no_script_and_no_request() {
        for dark in [true, false] {
            let page = page_html(dark);
            assert!(page.contains(
                "default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; \
                 img-src data:; font-src data:"
            ));
            assert!(!page.contains("<script"), "no script in the page");
            assert!(page.contains(page_background(dark)));
            assert!(page.contains("id=\"out\""));
        }
        assert_ne!(page_background(true), page_background(false));
    }

    #[test]
    fn the_user_script_is_mermaid_set_up_strict_in_the_theme() {
        for (dark, theme) in [(true, "dark"), (false, "default")] {
            let script = user_script(dark);
            assert!(script.starts_with(MERMAID_JS), "the bundled script");
            assert!(script.contains("startOnLoad: false"));
            assert!(script.contains("securityLevel: 'strict'"));
            assert!(script.contains(&format!("theme: '{theme}'")), "{theme}");
            assert_eq!(mermaid_theme(dark), theme);
        }
    }

    #[test]
    fn only_the_page_s_own_load_may_navigate() {
        assert!(navigation_allowed(true, "about:blank"));
        assert!(!navigation_allowed(false, "about:blank"), "a frame");
        for url in [
            "http://127.0.0.1:8080/",
            "https://example.com",
            "file:///etc/hosts",
            "about:srcdoc",
            "data:text/html,x",
            "",
        ] {
            assert!(!navigation_allowed(true, url), "{url}");
        }
    }

    #[test]
    fn the_cache_also_keeps_within_a_byte_budget() {
        let mut lru = Lru::with_budget(10, 100);
        assert!(lru.put_weighted(1, "a", 40).is_empty());
        assert!(lru.put_weighted(2, "b", 40).is_empty());
        assert_eq!(lru.weight(), 80);
        // 50 more: over 100, so the oldest goes.
        assert_eq!(lru.put_weighted(3, "c", 50), vec![(1, "a")]);
        assert_eq!(lru.weight(), 90);
        // One heavier than the budget stays alone (the newest always stays).
        assert_eq!(lru.put_weighted(4, "d", 500), vec![(2, "b"), (3, "c")]);
        assert_eq!((lru.len(), lru.weight()), (1, 500));
        // Replacing frees the old weight.
        assert!(lru.put_weighted(4, "D", 10).is_empty());
        assert_eq!(lru.weight(), 10);
    }

    fn k(n: u8) -> Key {
        Key {
            sha: [n; 32],
            dark: false,
        }
    }

    /// Review fix: the store's bookkeeping with a fake render step.
    #[test]
    fn a_diagram_is_queued_once_and_each_asker_told_once() {
        let t = Instant::now();
        let mut store: Store<&str, &str> = Store::new(64, usize::MAX);
        assert_eq!(store.ask(k(1), "a", 7, "tab 7", t), Asked::Waiting);
        // Asked again each frame, and by a second tab: still one job, two to tell.
        assert_eq!(store.ask(k(1), "a", 7, "tab 7 again", t), Asked::Waiting);
        assert_eq!(store.ask(k(1), "a", 8, "tab 8", t), Asked::Waiting);
        assert_eq!(store.counts().2, 1, "queued once");
        let job = store.next_job(|_, _| true);
        assert_eq!(job, Some((k(1), "a".to_string())));
        assert_eq!(store.next_job(|_, _| true), None);
        let (told, freed) = store.finish(k(1), Outcome::Drawn("img", 10), t);
        assert_eq!(told, ["tab 7", "tab 8"]);
        assert!(freed.is_empty());
        assert_eq!(store.ask(k(1), "a", 7, "x", t), Asked::Ready("img"));
        assert_eq!(store.counts(), (1, 10, 0));
    }

    #[test]
    fn a_job_nobody_wants_any_more_is_dropped() {
        let t = Instant::now();
        let mut store: Store<&str, &str> = Store::new(64, usize::MAX);
        store.ask(k(1), "a", 1, "closed tab", t);
        store.ask(k(2), "b", 1, "closed tab", t);
        store.ask(k(2), "b", 2, "open tab", t);
        store.ask(k(3), "c", 2, "open tab", t);
        // Tab 1 is gone (or its file and theme no longer have the diagram).
        let mut wanted = |_: &Key, w: &&str| *w != "closed tab";
        assert_eq!(store.next_job(&mut wanted), Some((k(2), "b".to_string())));
        let (told, _) = store.finish(k(2), Outcome::Drawn("img", 1), t);
        assert_eq!(told, ["open tab"], "only the tab that still wants it");
        assert_eq!(store.next_job(&mut wanted), Some((k(3), "c".to_string())));
        assert_eq!(store.next_job(&mut wanted), None);
        // Dropped, not cached: asked for again, it's queued again.
        assert_eq!(store.ask(k(1), "a", 3, "new tab", t), Asked::Waiting);
        assert_eq!(store.counts().2, 1);
    }

    #[test]
    fn mermaid_errors_are_kept_and_webkit_errors_retried_once() {
        let t = Instant::now();
        let mut store: Store<&str, &str> = Store::new(64, usize::MAX);
        store.ask(k(1), "bad", 1, "tab", t);
        store.next_job(|_, _| true);
        let (told, _) = store.finish(k(1), Outcome::Mermaid("Parse error".into()), t);
        assert_eq!(told, ["tab"]);
        assert_eq!(
            store.ask(k(1), "bad", 1, "tab", t),
            Asked::Failed("Parse error".into())
        );
        assert_eq!(store.counts().2, 0, "not rendered again");

        store.ask(k(2), "slow", 1, "tab", t);
        store.next_job(|_, _| true);
        let (told, _) = store.finish(k(2), Outcome::WebKit("timed out".into()), t);
        assert_eq!(told, ["tab"], "told, so it asks again");
        // Not kept: asked again, it renders once more.
        assert_eq!(store.ask(k(2), "slow", 1, "tab", t), Asked::Waiting);
        assert_eq!(
            store.next_job(|_, _| true),
            Some((k(2), "slow".to_string()))
        );
        store.finish(k(2), Outcome::WebKit("timed out".into()), t);
        // A second WebKit failure is kept: no endless retries.
        assert_eq!(
            store.ask(k(2), "slow", 1, "tab", t),
            Asked::Failed("timed out".into())
        );
        assert_eq!(store.counts().2, 0);
    }

    #[test]
    fn a_diagram_dropped_for_room_isn_t_drawn_again_at_once() {
        let t = Instant::now();
        // Room for two images of 10 bytes.
        let mut store: Store<&str, &str> = Store::new(64, 20);
        for (n, img) in [(1, "one"), (2, "two")] {
            store.ask(k(n), "src", 1, "tab", t);
            store.next_job(|_, _| true);
            store.finish(k(n), Outcome::Drawn(img, 10), t);
        }
        store.ask(k(3), "src", 1, "tab", t);
        store.next_job(|_, _| true);
        let (_, freed) = store.finish(k(3), Outcome::Drawn("three", 10), t);
        assert_eq!(freed, ["one"], "its image freed");
        // Still on screen, asked for at once: code with a note, not a render loop.
        assert_eq!(store.ask(k(1), "src", 1, "tab", t), Asked::TooMuch);
        assert_eq!(store.counts().2, 0);
        // After the pause it may render again.
        let later = t + REDRAW_PAUSE + Duration::from_secs(1);
        assert_eq!(store.ask(k(1), "src", 1, "tab", later), Asked::Waiting);
    }

    #[test]
    fn the_render_script_carries_the_source_as_a_string_and_the_theme() {
        let src = "graph TD\n  A[\"</script> `x` ${y}\"] --> B\u{2028}";
        let js = render_script(src, true);
        // The source goes in as a JSON string: quotes, backticks and line breaks escaped.
        assert!(js.contains(&serde_json::to_string(src).unwrap()));
        assert!(js.contains("mermaid.render("));
        assert!(js.contains("theme: 'dark'"));
        assert!(js.contains("securityLevel: 'strict'"));
        assert!(render_script(src, false).contains("theme: 'default'"));
    }

    #[test]
    fn the_answer_is_a_size_or_an_error() {
        assert_eq!(read_answer(r#"{"w": 320.5, "h": 200}"#), Ok((320.5, 200.0)));
        assert_eq!(
            read_answer(r#"{"error": "Parse error on line 1"}"#),
            Err("Parse error on line 1".to_string())
        );
        assert!(read_answer("nonsense").is_err());
        assert!(
            read_answer(r#"{"w": 0, "h": 10}"#).is_err(),
            "an empty diagram"
        );
        assert!(read_answer(r#"{"w": 10}"#).is_err());
    }

    #[test]
    fn snapshots_are_two_x_unless_huge() {
        assert_eq!(density(800.0, 600.0), 2.0);
        assert_eq!(density(4096.0, 100.0), 2.0);
        // A side over 4096 points: 8192 pixels at most.
        assert_eq!(density(8192.0, 100.0), 1.0);
        assert_eq!(density(100.0, 16384.0), 0.5);
        // A large square: the area cap.
        let d = density(4000.0, 4000.0);
        assert!((4000.0 * d * 4000.0 * d) <= MAX_AREA_PX + 1.0, "{d}");
        assert!(d > 1.0);
    }
}
