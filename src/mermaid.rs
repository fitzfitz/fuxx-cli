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
/// `cap` drops (and returns) the oldest.
pub struct Lru<K, V> {
    cap: usize,
    /// Oldest first.
    entries: VecDeque<(K, V)>,
}

impl<K: PartialEq, V> Lru<K, V> {
    pub fn new(cap: usize) -> Self {
        Lru {
            cap,
            entries: VecDeque::new(),
        }
    }

    #[cfg(any(test, feature = "ui-shots"))]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The value for `key`, now the newest.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let at = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(at)?;
        self.entries.push_back(entry);
        self.entries.back().map(|(_, v)| v)
    }

    /// Keep `value` for `key` (replacing any), as the newest; the entries dropped to stay
    /// within `cap`, oldest first.
    pub fn put(&mut self, key: K, value: V) -> Vec<(K, V)> {
        if let Some(at) = self.entries.iter().position(|(k, _)| *k == key) {
            self.entries.remove(at);
        }
        self.entries.push_back((key, value));
        let mut dropped = Vec::new();
        while self.entries.len() > self.cap {
            dropped.extend(self.entries.pop_front());
        }
        dropped
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

/// The page the hidden web view loads (with no base URL): a Content-Security-Policy that
/// allows no request at all (only the inline script and styles, and `data:` images and
/// fonts), the bundled mermaid.js inline, and Mermaid set up to draw only when asked
/// (`startOnLoad: false`), in `strict` security, in the theme for `dark`.
pub fn page_html(dark: bool) -> String {
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="{CSP}">
<style>
html, body {{ margin: 0; padding: 0; background: {bg}; }}
#out {{ position: absolute; left: 0; top: 0; display: inline-block; padding: 16px; background: {bg}; }}
</style>
<script>{MERMAID_JS}</script>
<script>{init}</script>
</head><body><div id="out"></div></body></html>"#,
        bg = page_background(dark),
        init = init_js(dark),
    )
}

/// The page's policy: nothing may be fetched (no `connect-src`, no remote script, style,
/// image, font, frame or media); only the page's own inline script and styles, and `data:`
/// images and fonts, which Mermaid makes itself.
const CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
                   img-src data:; font-src data:";

/// Mermaid set up for the theme for `dark`: drawn only when asked, in strict security (no
/// click handlers or scripts in labels, HTML in them sanitised).
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

    #[test]
    fn the_page_has_the_script_strict_security_no_network_and_the_theme() {
        for (dark, theme) in [(true, "dark"), (false, "default")] {
            let page = page_html(dark);
            assert!(page.contains(MERMAID_JS), "the bundled script, inline");
            assert!(page.contains("startOnLoad: false"));
            assert!(page.contains("securityLevel: 'strict'"));
            assert!(page.contains(&format!("theme: '{theme}'")), "{theme}");
            assert_eq!(mermaid_theme(dark), theme);
            assert!(page.contains(page_background(dark)));
            // No request leaves the page: only the inline script and styles, data: images.
            assert!(page.contains(
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
                 img-src data:; font-src data:"
            ));
            assert!(!page.contains("src=\"http"), "no remote script");
        }
        assert_ne!(page_background(true), page_background(false));
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
