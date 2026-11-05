---
tags: [development, plan, milestone]
milestone: M10c
status: planned
updated: 2026-11-06
---

# M10c Plan — Agent Browser (a scriptable browser window per session)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[52 - Milestone 10c - Agent Browser]] (the spec) · design evidence: [[Input - M10 Feasibility Study]] §4 (option c + automation API) |
| **Status** | ⬜ Planned (2026-11-06) |
| **Tasks done** | 0 / 7 |
| **▶ Pick up here** | Task 0 |
| **Decisions used** | Yours: for agents to automate (2026-10-27); autonomous run (2026-11-05). Everything else is the AI's call. |
| **Needs your eyes** | 👀 Task 6 (batched with M10b and M11) |

**Goal:** A per-session WebKit browser window that agents drive through `fuxx browser …` and `fuxx mcp`: open, snapshot
with refs, click, fill, press, read, eval, wait, screenshot, url, back, reload, close.

**Architecture:** A macOS-only `browser.rs` wraps one `NSWindow` + `WKWebView` per session through objc2 (main thread
only; every WebKit answer comes back through a channel to a GPUI task). The page-side work is JavaScript kept in
`browser_js.rs` (pure Rust strings and parsers, unit-tested). `workspace_browser.rs` owns the session ↔ browser map,
the menu item and the address box. The M9 socket gains `browser.*` methods; the CLI and MCP map onto them. A hidden
`fuxx __browser-probe` self-test drives a real off-screen web view end to end, so agents can verify WebKit behaviour
without a visible window.

**Tech Stack:** New crate `objc2-web-kit 0.3.2` (matches the `objc2 0.6.4` / `objc2-app-kit 0.3.2` /
`objc2-foundation 0.3.2` / `block2 0.6.2` already in `Cargo.lock` through GPUI; verified on crates.io 2026-11-06).
Those four become direct dependencies at the versions already locked. No other new crates.

**Spec:** [[52 - Milestone 10c - Agent Browser]] · [[12 - Scope]]

## Global Constraints

- Only `http` and `https`. Normalising: a bare host gets `http://` when it is `localhost`, `*.localhost`, an IPv4 or
  bracketed IPv6 address; otherwise `https://`. `file:`, `javascript:`, `data:`, `about:` (except internal
  `about:blank`) and anything else are refused with `only http and https addresses can be opened`.
- Every WebKit/AppKit call runs on the main thread. Nothing ever blocks the UI thread waiting for WebKit.
- Time limits: page load in `open` 30 s; `eval` 10 s; `wait` default 30 s (max 300 s), polling every 200 ms.
- Arguments placed into JavaScript are embedded only as JSON literals (`serde_json::to_string`), never by string
  concatenation of raw text.
- Refs are `e<N>`, stored on elements as the attribute `data-fuxx-ref`, renumbered by each `snapshot`.
- Window: 1100 × 800 points, cascaded from fuxx's window, title `<session name> — <page title>` (just the session name
  until a title exists). `isInspectable = true` where macOS supports it.
- One shared persistent website data store (`WKWebsiteDataStore.defaultDataStore`) for all fuxx browsers.
- `window.open(url)` loads in the same window (a document-start user script).
- Shortcut **⇧⌘B** for File → "Open Browser…".
- Errors (exact): `no browser for <sN> (fuxx browser open URL first)`, `unknown ref <ref> (take a new snapshot)`,
  `the page didn't answer in <N> s`, `the page failed to load: <reason>`.
- Browsers belong to the GUI process: with the window closed, `browser.*` returns the existing "no window" error.
- Non-browser behaviour is unchanged; every existing test passes unchanged.

## Review Focus

1. **Text with quotes, backslashes, newlines or `</script>` passed to fill/eval/wait breaks out of the script** →
   arguments embedded as JSON literals (Task 2 tests with hostile strings).
2. **A page that never finishes loading, or JavaScript that never returns, hangs the command or the UI** → every call
   has a time limit and the UI thread never waits (Task 3 probe cases; Task 5 test with a stub).
3. **Closing the session or the browser window while a command is pending crashes or leaks** → the pending command
   errors, the entry is removed (Task 4 test on the map; Task 3 probe case).
4. **`javascript:` / `file:` / `data:` addresses get loaded** → refused before WebKit sees them (Task 1).
5. **Snapshot refs go stale after navigation and click the wrong element** → refs are attributes on the elements
   themselves, so a stale ref is "unknown", never a different element (Task 2 JS + Task 3 probe case).

---

## Files

- Create: `src/browser_url.rs`, `src/browser_js.rs`, `src/browser.rs`, `src/workspace_browser.rs`.
- Modify: `Cargo.toml`, `src/main.rs`, `src/cli.rs`, `src/workspace.rs`, `src/workspace_api.rs`, `src/mcp.rs`.

### Task 0: Spike — WebKit from fuxx's process (hidden probe) — ⬜

**Files:** `Cargo.toml`, `src/browser.rs` (first cut), `src/main.rs`, `src/cli.rs` (hidden `__browser-probe`).

- [ ] Add the crates per Tech Stack. Look up the exact `objc2-web-kit 0.3.2` feature names and APIs on docs.rs (Context7
  has no entry); record them in the report.
- [ ] `fuxx __browser-probe` starts a GPUI application with no fuxx window, makes an off-screen 800 × 600 `NSWindow`
  with a `WKWebView`, loads a built-in test page with `loadHTMLString` (a heading, a link, a text field, a button that
  changes the heading), then: evaluates `document.title`; fills and clicks via JS; reads the heading; takes a snapshot
  to PNG in a temp dir; prints one JSON report line (`{"eval":…, "clicked":…, "png_bytes":N, "ms":…}`) and exits 0.
  Hard exit after 15 s with code 2. Hidden from `fuxx help`.
- [ ] Agents may run this probe (it opens no visible window and exits by itself); it is the only GUI process they run.
- [ ] Test: `the_browser_probe_parses_and_stays_out_of_help`. Run the probe; paste its output in the report.
- [ ] Commit `spike(m10c): WebKit probe — load, script and capture a web view from fuxx`.

### Task 1: Addresses — ⬜

**Files:** Create `src/browser_url.rs`.

- `pub fn normalize_url(input: &str) -> Result<String, String>` per Global Constraints (trim; keep an explicit
  `http(s)://`; add a scheme to a bare host[:port][/path]; refuse other schemes and empty input; refuse whitespace or
  control characters inside).
- [ ] Tests: `localhost_and_ips_get_http` (`localhost:3000`, `127.0.0.1:8080/x`, `[::1]:5173`, `app.localhost`),
  `other_hosts_get_https` (`example.com`, `docs.rs/objc2`), `explicit_schemes_are_kept`, `other_schemes_are_refused`
  (`javascript:alert(1)`, `file:///etc/passwd`, `data:text/html,x`, `ftp://h`, `about:config`), `blank_and_spaces_are_refused`.
- [ ] RED → implement → GREEN; full suite; commit `feat(m10c): browser addresses are http(s) only`.

### Task 2: The page-side scripts — ⬜

**Files:** Create `src/browser_js.rs`.

**Interfaces — Produces:**
- `pub fn snapshot_js() -> &'static str` — walks the visible DOM; numbers interactive elements (links, buttons,
  inputs, selects, textareas, `[role=button]`, `[onclick]`, `[contenteditable]`) with `data-fuxx-ref="eN"` (clearing
  old refs first); returns JSON `{ "url", "title", "nodes": [ { "role", "name", "ref"?, "value"?, "level"? } ] }` with
  headings, landmarks, text blocks (trimmed, ≤ 200 chars each) and controls; at most 1,000 nodes.
- `pub fn click_js(r: &str) -> String`, `fill_js(r: &str, text: &str) -> String` (sets value, dispatches `input` and
  `change`), `press_js(key: &str) -> String` (keydown/keypress/keyup on `document.activeElement`; Enter submits the
  form the element belongs to if no handler prevented it), `read_js() -> &'static str` (`document.body.innerText`,
  ≤ 100,000 chars), `wait_check_js(text: Option<&str>, selector: Option<&str>) -> String` (returns `true`/`false`),
  `window_open_shim() -> &'static str`.
- Each ref-taking script returns `{ "ok": true }` or `{ "error": "unknown ref eN" }`.
- `pub struct Snapshot { pub url: String, pub title: String, pub nodes: Vec<Node> }`, `pub struct Node { role, name,
  r#ref: Option<String>, value: Option<String>, level: Option<u8> }` (serde); `pub fn render_outline(s: &Snapshot) ->
  String` — one line per node, indented by heading level, controls as `[eN] role "name"` (+ `= "value"`).
- [ ] Tests: `arguments_are_embedded_as_json` (hostile strings `"`, `\`, newline, `</script>`, `'); alert(1); ('` appear
  only inside a JSON string literal of the produced script), `snapshot_json_parses` (a recorded sample),
  `outline_renders_refs_and_headings`, `node_and_text_caps` (render truncation).
- [ ] RED → implement → GREEN; full suite; commit `feat(m10c): page scripts for snapshot, click, fill, press, read and wait`.

### Task 3: The browser window — ⬜

**Files:** `src/browser.rs` (finish), `src/main.rs`/`src/cli.rs` (extend the probe).

**Interfaces — Produces (macOS, main thread):**
- `pub struct Browser` — owns the `NSWindow` and `WKWebView`; `pub fn open(title: &str, cx: &mut App) -> Browser` (shown,
  1100 × 800, cascaded; user script `window_open_shim` at document start; `isInspectable`);
  `pub fn load(&self, url: &str)`; `pub fn eval(&self, js: &str) -> oneshot Receiver<Result<serde_json::Value, String>>`
  (completion handler sends; the caller applies the 10 s limit); `pub fn state(&self) -> PageState { url, title,
  loading: bool, error: Option<String> }` (read from `WKWebView` properties); `pub fn screenshot(&self) -> Receiver<Result<Vec<u8>, String>>`
  (PNG); `pub fn back(&self)`, `reload(&self)`, `set_title(&self, t: &str)`, `bring_to_front(&self)`, `close(self)`;
  `pub fn is_closed(&self) -> bool` (the user closed the window).
- Load errors: detect with `state()` polling (no navigation delegate in M10c; AI's call): `loading` false and URL still
  `about:blank` after a load request → `the page failed to load: <url>`.
- [ ] The probe now runs every operation through `Browser` (load an HTML string page; eval; fill/click via `browser_js`;
  `snapshot_js` parsed into `Snapshot`; screenshot; `back`; a script that never returns hits the caller's timeout;
  close mid-eval → the receiver errors, no crash) and prints one JSON report.
- [ ] Unit tests where pure; the probe output pasted in the report is this task's evidence.
- [ ] Commit `feat(m10c): a WebKit browser window, scripted through channels`.

### Task 4: Browsers in the workspace — ⬜

**Files:** Create `src/workspace_browser.rs`; modify `workspace.rs`, `main.rs`.

- `Workspace.browsers: HashMap<SessionId, Browser>` (use the existing session id type).
- `Workspace::browser_open(&mut self, index, url: &str, window, cx) -> Result<(), String>` — normalise, create or reuse,
  load, bring to front, title `<session name> — <page title>` kept up to date (a 500 ms GPUI timer per open browser
  updates the title and notices a user-closed window, removing the entry).
- Closing a session closes and removes its browser; a renamed session retitles it.
- File → "Open Browser…" (⇧⌘B, action `OpenBrowser`) shows an address box over the window (reuse the host picker's
  overlay pattern / `NameInput`), placeholder `Address — localhost:3000 or a URL`; Enter opens for the active session;
  an invalid address shows the error in the box.
- Pure, tested: `pub fn browser_title(session: &str, page: Option<&str>) -> String`; the map's remove-on-close logic as
  a small struct with a test (`closing_a_session_drops_its_browser`, `a_user_closed_window_is_forgotten`).
- [ ] RED → implement → GREEN; full suite; commit `feat(m10c): a browser per session, File → Open Browser (⇧⌘B)`.

### Task 5: `fuxx browser …` and the MCP tools — ⬜

**Files:** `workspace_api.rs`, `cli.rs`, `mcp.rs` (+ `workspace_browser.rs`).

- Socket methods `browser.open {target?, url}`, `browser.snapshot`, `browser.click {ref}`, `browser.fill {ref, text}`,
  `browser.press {key}`, `browser.read`, `browser.eval {js}`, `browser.wait {text?|selector?, timeout_ms?}`,
  `browser.screenshot {path?}`, `browser.url`, `browser.back`, `browser.reload`, `browser.close` — `target` defaults to
  the caller's session as other M9 methods do. Replies are deferred (like M9 `wait`) and resolved from GPUI tasks;
  limits per Global Constraints; exact error texts.
- `open` replies after load (or the 30 s error); `snapshot` replies with the parsed `Snapshot` (JSON) and the CLI prints
  `render_outline`; `screenshot` writes the PNG (default `$TMPDIR/fuxx-browser-<sN>-<unix ms>.png`) and replies with
  the path.
- CLI: `fuxx browser <open URL|snapshot|click REF|fill REF TEXT|press KEY|read|eval JS|wait (--text T|--selector S)
  [--timeout S]|screenshot [PATH]|url|back|reload|close> [--session sN] [--json]`.
- MCP tools: `browser_open`, `browser_snapshot`, `browser_click`, `browser_fill`, `browser_press`, `browser_read`,
  `browser_eval`, `browser_wait`, `browser_screenshot`, `browser_url`, `browser_back`, `browser_reload`, `browser_close`,
  each with a one-line description and JSON schema in the existing style.
- [ ] Tests: `browser_commands_parse`, `browser_requests_map_to_methods` (CLI → method/params),
  `mcp_browser_tools_map`, `wait_needs_text_or_selector`, `a_command_without_a_browser_errors` (API with no browser:
  exact text), `timeouts_answer_with_the_exact_text` (a stub eval that never answers).
- [ ] RED → implement → GREEN; full suite; commit `feat(m10c): fuxx browser commands and MCP tools`.

### Task 6: On screen — 👀 ⬜ (batched with M10b and M11)

The spec's success check, plus: ⇧⌘B address box (valid / invalid address); title follows the page; ⌘C/⌘V/⌘A in a
page's text field; closing the browser window leaves the session; closing the session closes the browser; a second
session's browser is separate; Safari → Develop → fuxx lists the page; logins survive a restart of fuxx.

### Task 7: Update the docs — ⬜

- [ ] Note 52 (built; on-screen pending); this plan + verification log + deferred minors; roadmap; tracker; Progress Log;
  STRUCTURE.md (new files); README ("Agent browser" section); the probe documented as a hidden self-test kept for future
  GPUI/WebKit bumps.

## Verification log

_(filled in as tasks complete)_

## Known limitations

- No navigation delegate in M10c: load failures are noticed by polling page state.
- Pop-ups load in the same window; no tabs, downloads or uploads.
- Browsers aren't restored after quitting; with the window closed, browser commands aren't available.

## Related

- [[52 - Milestone 10c - Agent Browser]] · [[Input - M10 Feasibility Study]] · [[40 - Roadmap]] · [[00 - Plan Tracker (Start Here)]]
