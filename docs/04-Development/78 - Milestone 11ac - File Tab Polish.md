# Milestone 11ac — File Tab Polish

**Status:** 🟨 built 2026-11-13 on branch `m11ac-file-tab-polish` (not merged, not pushed); waiting for your
on-screen check (below). Built in parallel with M11p's planning. **Part 2 — Mermaid diagrams** 🟨 built 2026-11-13
on branch `m11ac-mermaid` (from master `e278c3f`; not merged, not pushed), waiting for your on-screen check
(Part 2's checklist, at the end).

**Asked for:** 2026-11-13, by you: "active file doesn't reflect/active on the file tab. If it's .md file, add 1 toggle
button like wrap called "Preview" so that the showing .md file is an md reader with rich format." Your choices the same
day: **"New M11ac, in parallel"** (its own milestone, built beside M11p's planning) and **"Highlight + reveal"** (the
Files panel selects the active file's row and opens its folders, like VS Code's "reveal active file").

## Goal

1. The right sidebar's **Files** panel shows which file you're looking at: when a file tab becomes the focused pane's
   tab, its row is selected, its folders open, and it's scrolled into view.
2. A Markdown file tab can be read as a document: a **Preview** toggle beside Wrap shows it rendered.

## What was built

**Files follows the active file tab.**
- When the focused pane's shown tab is a file tab (File or Diff mode, a preview tab too), and it's a different file
  than last time (or the same one after a terminal had the focus), the Files tree selects its row, opens its folders
  (listing the ones not listed yet, off the UI thread), and scrolls it into view once it shows.
- A file outside the tree's folder changes nothing. A terminal (or a browser tab) taking the focus keeps the last
  selection.
- It counts however the tab came up: a click on the tab, ⌃Tab / ⌃⇧Tab, opening a file (Files, Changes, ⌘P).
- While the same tab stays focused, the tree is yours: the arrow keys and clicks in it aren't overridden.

**Markdown Preview.**
- For a `.md` or `.markdown` file (any case), in **File** mode, the toolbar shows **Preview** right of **Wrap**,
  drawn the same way (outlined when off, filled when on). Tooltip: "Show the Markdown rendered". Hidden in Diff mode.
- Off by default; remembered per tab across restarts (`"preview": true` on the tab in `sessions.json`; older files
  load with it off, and a tab with it off is written exactly as before).
- When on, the tab shows a read-only reader instead of the source: headings sized by level (H1 and H2 underlined),
  paragraphs wrapped to the tab's width, bullet and numbered lists nested by indent (• ◦ ▪), task lists (☐ / ☑),
  fenced code blocks (the terminal's font on a tinted fill, with a border), inline code, **bold**, *italic*, links
  (underlined, in the accent colour, clickable), block quotes (a bar at the left, dim text), tables (a grid, the header
  filled and bold, columns aligned as `:--`, `:-:`, `--:` say), horizontal rules, and images as their alt text in
  brackets (`[logo]`).
- Links: `http(s)://` and `mailto:` open in your default browser; a link to a file next to it (`docs/guide.md`,
  `../src/main.rs#L3`) opens that file in a tab, if it exists; `#anchors` and other schemes do nothing.
- When the file changes on disk (the tab already re-reads it every second while shown), the Preview draws the new text
  and keeps its place (by block: the same block at the top, clamped if the text got shorter).
- Preview is offered for a coloured text **up to 1 MB**; above that (or a file too large to colour, 2–20 MB) the
  button is faint, does nothing, and its tooltip says "Too large to preview" (review fix).
- **⌘F** while in Preview turns Preview off and opens the find bar on the source.
- Colours come from the theme; the text, dim text, links and code text are held to the 4.5:1 text floor
  (`TEXT_MIN`) in every built-in theme (a test checks it).

## How it's built

- `src/agent_markdown.rs` (M11aa's parser) extended **additively**: `Span::Italic`, `Span::Image`; `Md::Task`,
  `Md::Nested(depth, item)`, `Md::Quote(blocks)`, `Md::Table { align, head, rows }`, `Md::Rule`; plus autolinks
  `<https://…>`, `\` escapes, setext headings (`Title` / `===`), closing `##`, an indented line continuing a list item.
  The four original tests pass unchanged (the agent subset parses as before; an unterminated fence or span is still
  safe while streaming). Pure, no new crate. Review fixes: block quotes nest at most **16** deep (a deeper `>` is text,
  so a crafted file can't overflow the stack), and the inline scanners are linear (italic closers found once per
  paragraph and searched; the looks for a link's `](` / `)` and an autolink's `>` stop at the next `[`, `<`, space or
  line break): 1 MB of unclosed markers parses in well under a second in a debug build.
- `src/md_preview.rs` (new): pure helpers (`is_markdown`, `link_target`, `inline`, `marker`, `gap_before`) and the
  GPUI drawing: each block a row (at most 860 pt wide) of a GPUI **`list`** (review fix: only the rows in sight are
  laid out and drawn, so scrolling a long file costs what's on screen; the tab keeps the `ListState`), `StyledText`
  with highlights and a font override for inline code, `InteractiveText` for clickable links (ids stable per block), a
  CSS-style `grid` for tables.
- `src/file_tab.rs`: the Preview button, `preview_shown`, the reader body (the file parsed again only for a new
  version), ⌘F turning it off, link following, and two new events to the workspace (`Preview(on)` saved and keeps the
  tab; `OpenFile(path)` for a file link).
- `src/layout_model.rs`: `Model::md_preview` / `set_md_preview` (a list of tab ids, like M11t's previews), saved on
  `SavedTab::File` as `#[serde(default)] preview`; cleared when an M11t preview tab shows another file or the tab
  closes; kept when the tab moves to another pane.
- `src/file_tree.rs`: `rel_in_root` and `FileTree::reveal_file`. `src/right_sidebar.rs`: `FilesPanel::follow`
  (once per change of followed file or root) and `Workspace::follow_active_file`, called **where the focused tab
  changes** (`Workspace::note_active`, which every activation goes through: one line in `workspace.rs`) and where a new
  tree arrives — never inside a frame (review fix: M11p's rule that nothing visible changes synchronously in
  `Workspace::render`). The frame only reads `FilesPanel::wants_follow`; if a route ever changed the focused tab
  without `note_active`, it defers the follow to after the frame (a safety net; the ui-shot checks it never fired).

## AI's calls

1. **Wrap stays visible in Preview** (it applies to the source; keeping it avoids the toolbar shifting when Preview
   turns on).
2. Turning Preview on or off **keeps an M11t preview tab** (as switching File ↔ Diff does), and turning it on closes an
   open find bar (find works on the source).
3. A preview tab (M11t) reused for **another file shows it as source**; a tab dragged to another pane keeps Preview.
4. Reveal scrolls with "nearest" (no jump when the row already shows). A folded-away root (the header's fold) stays
   folded: the row is selected but not shown until you unfold it.
5. If the file's path isn't under the tree's folder as written, its real path (symlinks resolved) is tried once.
6. Code in the Preview (and a table's header) is drawn on **a tint of the text over the page** instead of `ui.panel`
   (in Catppuccin Latte the panel colour is all but the page, so code didn't show); the code in the terminal's font at
   13 pt; prose is 14 pt at 1.5 line height; table columns are equal widths. (Accepted by the controller at review.)
7. Links to files open in the focused pane as an ordinary (kept) tab, showing source.
8. A soft line break inside a paragraph is a space (as Markdown renders it); a hard break (two trailing spaces) isn't
   kept.
9. Preview's limits from the review: 1 MB of coloured text, quotes 16 deep.

## Limits

- Not understood (shown as text): HTML in the Markdown (`<p align="center">`, `<img>`, `<details>`), indented
  (4-space) code blocks, reference-style links `[a][1]`, footnotes, strikethrough, math (Mermaid: Part 2), a span inside bold
  or italic (`**bold `code`**` shows the backticks). Images aren't loaded, only their alt text.
- The Preview's text can't be selected or copied (⌘C does nothing there); turn Preview off to copy.
- Code blocks aren't syntax-coloured, and long code lines wrap rather than scroll sideways.
- Table columns share the width equally (a very wide table squeezes; no sideways scroll).
- Preview only up to 1 MB of text (a bigger Markdown file shows its source); the parse (linear) runs on the UI thread
  once per version read.
- Quotes deeper than 16 show their remaining `>` as text.
- An ignored (hidden) file can't be shown in the tree, so revealing it opens its folders and selects nothing visible.
- **Effect on agent text (M11aa)**: the parser is shared with the agent chat (not wired yet), so its additions apply
  there too. `\` before punctuation escapes it in prose (`..\..\x` reads `....\x`, `\\server` reads `\server`);
  `*` between characters makes italics (`2*3*4` shows the 3 in italics; `src/*.rs and tests/*.rs` italicises
  `.rs and tests/`); a line starting with `>` is a quote and `---` under a line makes it a heading. While a reply
  streams, a setext heading (`Title` then `===`) and a table (its header before its `|---|` line) first show as plain
  text and then change shape (a flicker) once the next line arrives. A link whose address holds a `[` no longer
  links (the linear scanner stops there): `[a](http://x/[1])`, `[local](http://[::1]:3000)` stay text.
- A small `.md` with one line over 20,000 characters (`COLOUR_LINE_MAX`; an inline base64 image, say) is "Too large to
  colour", so it gets no Preview: the button says "Too large to preview" though the file is tiny.
- One very large block (a huge paragraph, table or code block) is a single row of the list, so its text is built
  again every frame while it is on screen (acceptable under the 1 MB cap).

## Verification

- TDD for the pure parts: the parser (7 new tests, the 4 old ones unchanged), `md_preview` (6), the saved tab with and
  without `preview` and its model round trip (3), `rel_in_root` and `reveal_file` (2), `FilesPanel::follow` (1),
  `shows_preview` (1); review fixes: quote depth (a line of 50,000 `>` on a 256 KB stack), 1 MB of unclosed markers,
  `preview_fits`, `wants_follow`, the table header's contrast. Each new behaviour test was run and seen failing first, except three guards written with their code: `shows_preview`, the colour floor, and the saved shape (it passed as soon as the field existed; the round-trip test beside it failed first).
- Gate (after the review fixes): `cargo test --bin fuxx` 1634 passed, 1 ignored (1610 before M11ac); `cargo clippy --all-targets` 30 (the baseline, none new);
  `cargo clippy --all-targets --features ui-shots` 32; `cargo fmt --check` clean.
- ui-shots (`cargo run --features ui-shots -- __ui-shots file-tab-md` and `… files-follow`):
  `file-tab-md-source-with-toggle-{mocha,latte}`, `file-tab-md-preview-{mocha,latte}`,
  `files-panel-follows-active-tab-{mocha,latte}`. The scenes also check (not shot): a `.rs` file has no Preview; a long
  file scrolled in Preview and rewritten on disk is parsed again with its scroll kept; ⌘F turns Preview off and opens
  find; Diff mode shows none; 64,000 `>` render in Preview; a file over 1 MB keeps its source when Preview is
  clicked; the tree follows ⌃Tab / ⌃⇧Tab, opening files in File and Diff mode, keeps its selection
  for a terminal and for a file outside the folder.
- **Not verified on screen**: the AI can't see the real window. Your check below is what confirms it.

## On-screen checklist (for you)

1. Open two or three files from different folders, then switch between their tabs (click and ⌃Tab): the Files panel
   selects each one's row, opening its folder and scrolling to it.
2. Open a file from outside the panel's folder (e.g. via ⌘P in another repo, or a file link): the panel's selection
   doesn't move.
3. Switch a file tab to **Diff** (or open one from Changes): the panel still follows it. Click a terminal tab: the
   selection stays where it was.
4. Open a README with tables, task lists and code blocks; click **Preview** (beside Wrap): it reads as a document;
   the button is filled. Click a web link (opens in your browser) and a link to another file (opens in a tab).
5. With Preview on, let an agent (or you, in another editor) edit the `.md`: the Preview updates within a second and
   stays where you were scrolled.
6. Quit and reopen fuxx: the tab still shows Preview.
7. Press **⌘F** in Preview: it goes back to the source with the find bar open.
8. Check Preview in a light theme (e.g. Catppuccin Latte) and a dark one: text, links, code and table lines all read
   clearly.

## Part 2 — Mermaid diagrams (added 2026-11-13 by you)

**Asked for:** 2026-11-13, by you, after the controller recommended rendering with fuxx's own WebKit and a bundled
mermaid.js (offline): "put/add it in M11ac as an additional milestone". Built on its own branch, `m11ac-mermaid`,
from master `e278c3f` (which has Part 1).

### What was built

- In Preview, a fenced code block whose info string's first word is `mermaid` (any case: `mermaid`, `Mermaid`,
  `` ``` mermaid {…} ``) is drawn as the diagram instead of its code.
- While it renders, the block shows as code (as Part 1 draws it). When it's ready, the image replaces it: at its
  natural size, scaled down to fit the Preview's column (never up), centred, a row of the Preview's list like any
  other block.
- A diagram Mermaid can't draw keeps its code, with one line under it in the dim text colour: `Mermaid: <short
  error>` (its first line; for a parse error, what it found instead: `Mermaid: Parse error on line 3: got 'EOF'`).
  So does one that takes more than 5 s (`Mermaid: the page didn't answer in 5 s`).
- At most **20** diagrams are drawn per file; the 21st on stay code with `Mermaid: more than 20 diagrams — showing
  code`. A diagram source over **50 KB** stays code with `Mermaid: too large`.
- Light or dark follows fuxx's theme (Mermaid's `default` or `dark` theme). Switching between a light and a dark
  theme draws the diagrams again; two dark themes share the dark rendering.
- Rendered diagrams are cached in memory by (SHA-256 of the source, light/dark), **64** at most, least recently
  used out first (its decoded image let go too). Scrolling and redrawing only read the cache; the same diagram in two
  files renders once.
- **No network, ever**: the page is loaded from a string with no base URL, behind a Content-Security-Policy of
  `default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:`; the
  navigation delegate allows only the page's own first load and cancels every other navigation; the web view has a
  fresh non-persistent data store and no UI delegate (no new windows); Mermaid runs with `securityLevel: 'strict'`
  (no click handlers or scripts in labels). The diagram is a picture: a `click` link in it does nothing.

### How it's built

- **Renderer: approach (a)**, WebKit's snapshot as a PNG. Approach (b), handing the SVG to GPUI, was ruled out from
  the pinned GPUI's source: it draws SVGs with `resvg`/`usvg` (`crates/gpui/src/svg_renderer.rs`), which loads system
  fonts but has no HTML, and Mermaid's default flowchart labels are HTML inside `<foreignObject>` (they'd come out
  blank), and its `<style>` relies on CSS `usvg` only partly reads. WebKit draws exactly what Mermaid meant.
- `assets/vendor/mermaid/`: **mermaid 12.2.0** `dist/mermaid.min.js` from the npm tarball, unchanged (SHA-256
  `a8a6a6f8…3c3b9f`, checked by a test), its MIT `LICENSE`, and a README with the version, source URL, npm integrity
  and SHA-256. Embedded with `include_str!`: **about 5.5 MB** more in the binary (the script verbatim).
- `src/mermaid.rs` (new, pure, tested): `is_mermaid`, `plan` (which blocks render, the 20 and 50 KB limits), `key`
  (SHA-256 through macOS's CommonCrypto, in libSystem: no crate), `Lru`, `page_html` (the CSP, the script inline,
  `mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme })`), `render_script` (the source passed
  as a JSON string, `mermaid.render(id, source)` into `#out`, its size answered), `read_answer`, `density`,
  `short_error`, `note`.
- `src/mermaid_render.rs` (new): one hidden `WKWebView` (in a borderless window that's never shown), made lazily on
  the first diagram, on the main thread; each diagram runs with `callAsyncJavaScript` and is snapshotted with
  `WKSnapshotConfiguration` (its rect, and a width giving 2 pixels per point; a huge diagram less, at most 8192
  pixels a side and 16.7 million in all). Every WebKit answer comes back through a completion block into a channel a
  GPUI task awaits: the UI thread never waits. One diagram at a time, the rest queued; a WebKit failure or a timeout
  throws the web view away and the next diagram gets a fresh one. `Mermaids` (a GPUI global) holds the cache, the
  queue and who's waiting (once per Preview).
- `src/md_preview.rs`: `Shown` (a diagram, or the code with an optional note) and `Draw::mermaid`; a Mermaid block's
  row is the image (`img` in a box of the image's aspect ratio, `max_w` its natural width, centred) or the code with
  the dim note. `src/file_tab.rs`: the plan made once per file version and light/dark; each frame asks the cache
  (`Mermaids::shown`), and a finished diagram re-measures the list's rows and redraws the tab.
- `Cargo.toml`: two more `objc2-web-kit` **features** (no new crate): `WKContentWorld` and `WKFrameInfo`, for
  `callAsyncJavaScript`. `src/browser.rs`: `png_bytes`, `png_size` and `exception_message` made `pub(crate)` for it.

### AI's calls

1. **Approach (a)**, the PNG snapshot (why above).
2. **The diagram sits on Mermaid's own page colour** — white for light, `#333` for dark (what Mermaid's themes are
   drawn for), with rounded corners — not on the theme's background. The cache key is light/dark only, so one
   rendering serves every dark (or light) theme; matching each theme's exact background would need it in the key.
3. **Mermaid's `dark` theme for every dark fuxx theme, `default` for every light one**, decided by the theme's
   background (`Theme::is_dark`).
4. **Counting to 20 is in document order over every Mermaid block**, a too-large one included; only top-level
   blocks are diagrams (a Mermaid block inside a block quote stays code).
5. **5 s per diagram** (render and snapshot), and **10 s more for the page's first load** (5.5 MB of script), so the
   first diagram isn't failed by the load.
6. **Errors are cached like diagrams** (a broken diagram isn't retried every frame); editing it makes a new key.
7. **Nothing renders until Preview shows the file**; all of its (first 20) diagrams are queued then, not only those
   in sight, so scrolling down finds them ready.
8. **A failure is logged** to stderr (`[fuxx] mermaid: …`, the whole message) as well as shown short.

### Limits

- About **5.5 MB** more binary (mermaid 12.2.0's single-file build). The first diagram after launch waits for the
  page to load its script (the first diagram took under a second in the ui-shots, load included); the page then
  stays loaded until fuxx quits, holding a WebKit content process's memory (not measured).
- Diagrams are pictures: no hover, no links, no text selection; a very tall diagram (beyond 4096 points a side, or
  16.7 million pixels) is snapshotted less densely and looks softer.
- Two dark themes show the same dark diagram, on `#333`, not on their own background.
- Mermaid features that need the network (icon packs, remote images or fonts, `@import`) draw without them.
- A Mermaid block in a quote stays code. More than 64 diagrams on screen at once (four Previews of 20) would render
  again in turn as the cache evicts them.
- The bundle's dependencies (d3, dagre, DOMPurify, lodash-es, cytoscape and others) ship inside it under their own
  licences (MIT, ISC, BSD, Apache-2.0/MPL-2.0 for DOMPurify), their banners kept in the file.

### Verification

- TDD for the pure parts (`src/mermaid.rs`, 16 tests, each seen failing first against stubs): which info strings are
  Mermaid (`mermaid`, `Mermaid`, `` ``` mermaid {…}`` through the parser), SHA-256 against known vectors, the key, the
  vendored file's SHA-256, the plan (20, 50 KB, light/dark), the note text, short errors (first line, parse errors,
  100 characters), the LRU (order, replacement, 64), the page (script inline, strict, the theme, the CSP, no remote
  script), the render script (source as a JSON string, theme), the answer, the snapshot density.
- Gate: `cargo test --bin fuxx` 1650 passed, 1 ignored (1634 before); `cargo clippy --all-targets` 27 and
  `--features ui-shots` 29 warnings with this toolchain (none in the changed code; one it raised, a complex type, was
  fixed); `cargo fmt --check` clean; ui-shots `file` (every `file*` scene) pass.
- ui-shots `file-tab-md-mermaid` (`cargo run --features ui-shots -- __ui-shots file-tab-md-mermaid`): **real WebKit
  runs in the off-screen harness** if its run loop is turned by hand (the harness never turns it). Turning it runs
  GPUI's deferred window closes from earlier scenes, whose apps are gone (they panic), so the rendered half runs in a
  child process, first and alone. Shots: `file-tab-md-mermaid-pending-{mocha,latte}` (just after Preview: all code,
  no notes) and `file-tab-md-mermaid-{mocha,latte}` (a flowchart scaled to the column, a sequence diagram at its
  natural size centred, a broken one as code with `Mermaid: Parse error on line 3: got 'EOF'`; rendered in under a
  second). Also checked, not shot: scrolling the Preview renders nothing again (cache 3, none waiting, before and
  after); switching light ↔ dark renders all three again (cache 6); and **no network**: a diagram with an `<img>` in
  a label, a `themeCSS` `@import` and a `click` link, all to a server on 127.0.0.1, renders and the server gets no
  connection (with the CSP taken out by hand, the same check does see a connection, so it's a real check).
- **Not verified on screen**: the AI can't see the real window. Your check below confirms it.

### On-screen checklist (for you)

1. Open a README with a flowchart and a sequence diagram (` ```mermaid `), turn **Preview** on: each shows as code
   for a moment, then as the diagram, centred, no wider than the Preview.
2. The same in a light theme (e.g. Catppuccin Latte) and a dark one: light diagrams on white, dark ones on dark grey,
   both readable.
3. Break a diagram (e.g. `A -->` with nothing after it) and save: within a second it goes back to code with
   `Mermaid: Parse error …` under it in dim text.
4. Switch the theme between light and dark with the Preview open: the diagrams are drawn again in the new look.
5. Scroll a long Markdown file with several diagrams up and down: smooth, no flicker back to code.
6. No network: a diagram with `click A href "https://example.com"` shows, and clicking it does nothing (no browser
   opens). (Optional: with Little Snitch or `nettop`, fuxx makes no connection when a diagram renders.)
7. A file with more than 20 diagrams: the 21st on stay code with `Mermaid: more than 20 diagrams — showing code`.
