# Milestone 11ac — File Tab Polish

**Status:** 🟨 built 2026-11-13 on branch `m11ac-file-tab-polish` (not merged, not pushed); waiting for your
on-screen check (below). Built in parallel with M11p's planning.

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
9. Preview's limits from the review: 1 MB of coloured text, quotes 16 deep.
7. Links to files open in the focused pane as an ordinary (kept) tab, showing source.
8. A soft line break inside a paragraph is a space (as Markdown renders it); a hard break (two trailing spaces) isn't
   kept.

## Limits

- Not understood (shown as text): HTML in the Markdown (`<p align="center">`, `<img>`, `<details>`), indented
  (4-space) code blocks, reference-style links `[a][1]`, footnotes, strikethrough, Mermaid/math, a span inside bold
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
  text and then change shape (a flicker) once the next line arrives.

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
