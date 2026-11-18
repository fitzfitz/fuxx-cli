# Milestone 11ai — Markdown Reader Upgrade

**Status:** 🟨 built and **merged to master and pushed 2026-11-18** (`f7707e9`), waiting for your on-screen check. All 12 plan tasks (0–11) built 2026-11-18, Tasks 0–10 reviewed ([[M11ai Plan - Markdown Reader Upgrade]]); the full ui-shots run and the whole-branch review done, **the final review's fixes are in**. Design approved 2026-11-17 on your go-ahead ("we already design it, layout is also built before. just go ahead"); its three open questions settled as AI's calls (below). Built **after M11ah** (code view upgrade), in sequence: both live in the file tab. Anything here that does not quote you is marked **AI's call**.

**Asked for:** 2026-11-16, by you, in the file-view mockups ("everything" → "your order is fine, go ahead"). Direction:
[[Proposal - File View Upgrades]], section "M11ai — Markdown reader upgrade (directions A + B + C, merged)":

> - **A · Comfortable reader:** a centred column (about 70 characters), bigger type and spacing, coloured code blocks
>   with their language and a **Copy** button, GitHub-style callouts (Note, Tip, Important, Warning, Caution), heading
>   anchors.
> - **B · Docs site:** an "On this page" outline that follows where you are (click to jump), a thin reading-progress
>   line.
> - **C · Looks like GitHub:** striped tables, front-matter shown as a small table, badges drawn as pills from their alt
>   text (offline).
> - A's centred column and C's full width don't fit together: the AI's call is A's column with C's tables, badges and
>   front-matter (told to you 2026-11-16).

## Why

The Preview (M11ac) shows a Markdown file rendered, but it reads like a plain dump: small type, a wide column, grey
code, `> [!NOTE]` shown as a quote with the marker in it, front matter drawn as a rule and a big heading, badges as
`[` `[build]` `](https://…)`. You cannot copy a code block (Preview has no selection), and a long doc has no map.

M11ai makes the Preview a good place to *read* a doc an agent wrote or will follow: a README, a plan, a design note.
Who it's for: you, reading Markdown in a file tab, often next to an agent.

## Goals

1. **Comfortable to read.** A centred column of about 70 characters, bigger type, more air.
2. **Code you can use.** Code blocks coloured as the File view colours code, with their language and a **Copy**
   button.
3. **GitHub's Markdown looks like GitHub.** Callouts, striped tables, front matter as a small table, badges as pills.
4. **Find your way in a long doc.** An "On this page" outline that marks where you are, click to jump; heading
   anchors (`#install` links work); a thin progress line.

## Non-goals and anti-goals

- **Not in it:** editing; selecting or copying prose in Preview (unchanged from M11ac; Copy is for code blocks only);
  find inside the Preview (⌘F still turns Preview off, as today); loading images or badges from the network or disk
  (pictures stay their alt text); HTML (`<img>`, `<p align="center">`, `<details>`: still text, as today); footnotes,
  strikethrough, reference links (the parser's gaps stay); math; a zoom or font-size setting for the reader; the
  reader look in agent chat messages (AI's call; see question 2); M11ah's code-view features (sticky line, folding,
  scrollbar marks) in Preview.
- **Anti-goals (a failure even if it works):**
  - Anything fetched. A badge is drawn from its alt text; its image URL is never opened.
  - A frame drop when scrolling a long doc, or a new always-on timer (M11p, note 77).
  - Tracking the outline by changing state while drawing (M11p's render rule).
  - The reader jumping by itself: a reload, colours arriving or a diagram arriving must keep your place.
  - Colours under the text floor in any theme.

## Constraints

- Read-only, offline, no new services, no new crates ([[12 - Scope]]). The parser stays our own
  `agent_markdown::parse` (M11ac); colours stay `syntect` + `two-face` via `src/highlight.rs`.
- The Preview is a GPUI `list` of blocks, one row per block, only the rows in sight drawn (`md_preview::row`). Up to
  **2,000 blocks** every row is measured, so the scrollbar is exact; above, it is counted in blocks of 45 pt (note 80).
  Preview only up to **1 MB** of text.
- The parse happens once per file version (today in `preview_body`), scroll kept by block on reload.
- M11p: no new always-on timer; nothing visible changes inside render; a one-shot timer is fine.
- M11al (UI Uplift 3) ships first and makes everything roomier. **Every size below is a named token**, not a literal
  in the drawing code, so it can follow M11al (AI's call). The Preview's side and top padding use the same token as
  the terminal's inner padding (M11al: about 28 / 22 pt).
- M11ah changes the File and Diff canvas. M11ai touches only the Preview path (`md_preview.rs`, the Preview parts of
  `file_tab.rs`) and adds to `highlight.rs` without changing what it does for File mode.
- Shared parser: `agent_markdown` also feeds agent chat (M11aa, M11ak). Parser changes are additive only.

## Approaches

### 1. Where the new Markdown features are recognised (callouts, front matter, badges, anchors)

- **A. A reader plan beside the parse (AI's pick).** Front matter is split off the text *before* `parse` (a pure
  `front_matter(text)`). After `parse`, one pure pass builds a per-version **reader plan**: the headings (block index,
  level, text, slug), which quotes are callouts, which paragraphs are badge rows, and each code block's colour key.
  The plan sits next to the parsed blocks and the Mermaid plan, made once per version. The parser learns one thing,
  additively: a linked image `[![alt](src)](href)` (today it falls apart into text). Chat is untouched.
- **B. Teach the parser everything** (`Md::Callout`, `Md::FrontMatter`, `Md::BadgeRow`). Cleaner types, but every new
  variant reaches agent chat's drawing (M11aa, M11ak in flight) and its tests. Dropped: wider blast radius for no
  gain.
- **C. Swap in `pulldown-cmark` (or `comrak`).** Full CommonMark/GFM. Dropped: a new crate, a rewrite of M11ac's
  parser and its tests, and chat's parser would split from the Preview's. Out of the written plan.

### 2. "On this page" follows where you are

- **A. Decide on scroll, from the list's state (AI's pick).** The Preview list already has a scroll handler (wheel,
  drag, jumps). On each scroll, the tab works out the current heading from the list's top block and the plan's
  heading list (a binary search), and redraws only if it changed. Works the same above 2,000 blocks. No timer.
- **B. Read each heading's position while painting.** Exact pixels, but it changes state during drawing. Dropped
  (M11p).
- **C. A timer that checks the scroll.** Dropped: an always-on timer while Preview shows.

### 3. Copy button and coloured code inside one block row

- **A. A header strip inside the code block's row (AI's pick).** The language on the left, **Copy** on the right, the
  coloured code under it. Plain GPUI elements in the same row; the button's id comes from the row index (as links'
  ids do today), so a press and its release meet. Colours come from a background job, per version.
- **B. Copy shown only on hover, floating over the code's corner.** Quieter, but you have to find it, and it needs
  hover state per block. Dropped.
- **C. Draw code blocks on a canvas like the File view.** Fast for huge blocks, but a second text drawer to keep in
  step with M11ah. Dropped: Preview blocks are small and text elements are fine.

## The design

### A · Comfortable reader

**The column (AI's calls).**
- Prose sits in one centred column at most **36 em** wide (about 70 characters of the UI font at 16 pt; the builder
  checks the count on a real paragraph and tunes the em, `READER_MEASURE`). Today's `COLUMN_MAX` (860 pt) goes.
- Code blocks, tables, Mermaid diagrams, callouts and front matter all stay **inside** the column (your told call:
  A's column with C's tables). A wide table wraps its cells; code wraps as today. See question 1.
- When the outline shows (B), the column centres in the space left of it.

**Type and spacing (AI's calls; all tokens):**

| What | Today | M11ai |
| --- | --- | --- |
| Body | 14 pt, line 1.5 | **16 pt, line 1.65** |
| Headings H1–H6 | 24 / 20 / 17 / 15 / 14 / 13 | **30 / 24 / 20 / 17 / 16 / 15** (H6 dim) |
| Space between blocks | 12 pt | **16 pt** (1 em); **32 pt above a heading**, 12 pt under it |
| List items | 4 pt apart | **6 pt** apart |
| Code (blocks and inline) | 13 pt, line 1.45 | **14 pt, line 1.5** |
| H1 / H2 underline | as today | kept, 8 pt below the text |

**Code blocks.**
- A **header strip**, 28 pt high, on the block's top edge, a shade darker than the code's fill: the language on the
  left in 12 pt dim text, exactly as the fence names it (`rust`, `ts`, `sh`); nothing when the fence names none.
  **Copy** on the right: a copy icon and the word, dim, brighter on hover. Click → the block's code is on the
  clipboard (as written in the file, tabs kept) and the button reads **Copied** for 1.5 s (a one-shot timer), then
  Copy again.
- **Colours** follow the File view's: the same `Highlighter` and the theme's syntax colours. The fence's first word
  picks the syntax (`highlight.rs` gains `highlight_lang(lang, text)`, using syntect's
  `SyntaxSet::find_syntax_by_token`, checked with Context7 2026-11-17). Unknown or no language: plain code colour.
- Colouring runs on the background executor, once per version, for every code block in the file. Until it lands, code
  shows uncoloured (as the File view does). Colours are kept in a small cache keyed by (language, code text, theme),
  so on a reload unchanged blocks keep their colours at once; only new or changed blocks wait.
- A Mermaid block drawn as a **diagram** has no header (as today). A Mermaid block shown as **code** (waiting,
  failed, too many) gets the header ("mermaid", Copy); its reason line stays under it.

**Callouts (GitHub alerts).**
- A top-level quote whose first line is exactly `[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]` or `[!CAUTION]`
  (any case, spaces around allowed) is a callout. Anything else (`[!FOO]`, a marker inside a list or a quote) stays a
  quote, as GitHub does.
- Drawn as: a 3 pt bar on the left in the kind's colour, a faint wash of that colour behind, a title line with a small
  icon and the word (**Note**, **Tip**, **Important**, **Warning**, **Caution**) in the kind's colour, semibold, then
  the body in normal text colour (a quote is dim; a callout is not). 12 pt inside padding, 6 pt corners. A callout
  with no body shows its title alone.
- Colours from the theme's palette: Note blue, Tip green, Important magenta, Warning yellow, Caution red; the bar and
  icon held to 3:1, the title to the text floor (4.5:1), in every theme.

```markdown
> [!WARNING]
> Run the migration before you deploy.
```

**Heading anchors.**
- Each heading gets GitHub's slug: lowercase, punctuation dropped except `-` and `_`, spaces to `-`; a repeat gets
  `-1`, `-2`. (`## Getting Started!` → `getting-started`.) Pure and tested.
- A link `[see Install](#install)` in the Preview now **jumps** to that heading (today it does nothing). An unknown
  anchor still does nothing.
- A link to another Markdown file with a part, `[setup](guide.md#install)`, opens that file in a tab **in Preview,
  scrolled to that heading** (AI's call; today it opens the source at the top; see question 3). A link without a part,
  or to a non-Markdown file, behaves as today.
- Hovering a heading shows a dim **`#`** in the margin to its left. Clicking it copies the link to that heading, as
  the path from the workspace's folder plus the slug (`docs/guide.md#install`); it reads **Copied** for 1.5 s. If
  M11ag settles a different path form for its Copy with path, this follows M11ag's (AI's call).
- A jump puts the heading 16 pt below the top of the Preview.

### B · Docs site

**"On this page" outline.**
- **What it lists:** headings of level 1–3, H3 indented 12 pt. If the file has exactly one H1 and it is the first
  heading, it is the title and is left out (AI's call). Shown only when there are **at least 2** entries.
- **Where (AI's calls):** a column **200 pt** wide on the right of the Preview, with a small dim caption
  "On this page". It stays put while the text scrolls and scrolls on its own if longer than the view. It shows when
  the Preview is wide enough for the column, a 48 pt gap and the outline (about 880 pt with M11al's padding; worked
  out from the tokens — built: 784 pt, see "Calls made while building").
- **Narrower:** the outline hides, and a small **list** icon button sits at the Preview's top-right corner (tooltip
  "On this page"). Click → the outline opens as a floating panel under it (the app's overlay panel). Clicking an entry
  jumps and closes it; **Esc** or a click outside closes it.
- **Entries:** 13 pt, 26 pt rows, one line each (long ones end in "…"; the full text in a tooltip). The current entry
  is in the accent colour with a 2 pt accent bar on its left; the rest dim; hover gets the panel hover fill.
- **Follows you:** the current entry is the last heading at or above the top of the view (with a 25% slack: a heading
  in the top quarter of the view counts as reached). Scrolled to the very end, it is the last heading that is in view
  (so a short last section can be current). Worked out on scroll (approach 2A), never while drawing.
- **Click to jump:** as an anchor jump (16 pt below the top). A jump is a scroll like any other: the scrollbar shows,
  the current entry updates.
- No new keyboard shortcuts (AI's call). Esc only closes the narrow panel.

**Reading-progress line.**
- A **2 pt** line along the top edge of the Preview (under the tab's bar), in the accent colour, its length the share
  of the doc scrolled past (0 at the top, full at the end). Read from the same reckoning as the Preview scrollbar
  (`preview_track`): exact up to 2,000 blocks, counted in blocks above.
- Hidden when the whole doc fits. No animation: it moves with the scroll.

### C · Looks like GitHub

**Striped tables.**
- Every other body row (the 2nd, 4th, …) gets a faint fill (a 3.5% tint of the text over the page). The header keeps
  today's fill, semibold. Cells get a little more room: 13 pt sides, 7 pt top and bottom. Lines, alignment and equal
  column widths as today (AI's calls).

**Front matter.**
- A YAML block at the very top of the file, between a first line `---` and the next `---` or `...`, is front matter.
  It is split off before parsing, so it no longer shows as a rule and a big heading (today's bug).
- Drawn first, as a small two-column table: key (dim, 13 pt) and value (13 pt), in the table's lines, no stripes,
  rounded. Top-level `key: value` pairs only; a nested or list value shows as its text joined into one line
  (`tags: [docs, setup]` → `docs, setup`; an indented `- item` list → `item, item`). Long values wrap.
- At most **20 rows**, then a last row "+ 6 more" (AI's call). A block that doesn't read as `key: value` lines shows
  as a plain code block labelled "front matter". No closing `---`: not front matter (the text reads as today).
- `+++` (TOML) front matter is not recognised (GitHub does not either; AI's call).

```markdown
---
title: Setup guide
status: draft
tags: [docs, setup]
---
```

**Badges as pills.**
- An image is a **badge** when its source looks like one (host `img.shields.io`, `badgen.net`, `badge.fury.io`,
  `codecov.io`, or a path ending in `badge.svg`), or when it sits in a paragraph made only of two or more images
  (linked or not) and spaces (AI's call). Any other image keeps today's look (its alt text, dim and italic).
- A paragraph of only badges is a **badge row**: real pills, wrapping, 6 pt apart. Each pill: 20 pt high, fully
  rounded, 12 pt text, 7 pt sides, on a soft fill. Alt text with `": "` becomes a **two-part pill**, label on the
  left, value on a slightly stronger fill (`license: MIT` → `license | MIT`). Empty alt text: the word "badge", dim.
- A linked badge is clickable and follows its link with today's rules (web in the browser, a file in a tab). Its
  image source is never opened.
- A badge inside a sentence can't be a box (text runs have no corners): it shows as its alt text on the pill fill.
- Colours are neutral (we don't know if "build" passed): no green or red guessed from words (AI's call).

```markdown
[![build: passing](https://img.shields.io/badge/build-passing-green)](https://example.com/ci) ![license: MIT](https://img.shields.io/badge/license-MIT-blue)
```

### How it works with the rest

- **Live reload.** A new version is parsed again with its reader plan (front matter, headings, slugs, callouts,
  badges). The scroll is kept by block, as today (front matter counts as block 0 when present). The current outline
  entry is worked out again. Unchanged code blocks keep their colours (the cache); a "Copied" label goes.
- **Find.** Unchanged: ⌘F turns Preview off and finds in the source.
- **Preview scrollbar (M11ae).** Unchanged and shared: the progress line and the outline read the same list state.
  Anchor and outline jumps show the scrollbar like any scroll.
- **Mermaid (M11ac).** Diagrams stay as they are, centred, at most the column's width. A diagram arriving remeasures
  rows as today; the outline's current entry is worked out again after it (a remeasure is not a scroll).
- **Follow and live marks (M11af).** Preview still shows no glow or marks; turning Preview on or off still ends
  Follow.
- **Big documents.** All of it is per version and linear; the outline is its own list (only rows in sight drawn).
  Above 2,000 blocks, current-entry tracking uses block indices, so it still works; the progress line is an estimate,
  as the scrollbar is. Bigger type and the code header make rows taller and add elements: the builder measures the
  full relayout of a 2,000-block file again and lowers `PREVIEW_MEASURE_ALL` if it is over a frame (about 11 ms on a
  release build). If parsing plus the plan for a 1 MB file takes over 8 ms on the UI thread, the parse moves to the
  background executor with the read.
- **Themes.** Every colour from the theme, held to the floors (text 4.5:1, marks 3:1). The every-theme contrast test
  grows to cover callout titles, pill text on both fills, the outline's current entry and striped rows. Light and
  dark shots for each new look.
- **Agent chat.** Unchanged. `md_preview::column` (chat) draws as today; the reader features are switched on only
  for the Preview (a flag in `Draw`).

## Calls on the open questions (AI's, on your go-ahead — for you to check)

1. **Width:** prose stays in the ~70-character column; **code blocks and tables may grow wider than the prose, up to
   the Preview's width**, centred on the column (a wide table scrolls sideways inside itself rather than squeezing).
   Reason: squeezed tables and wrapped code are the hardest things to read; GitHub also lets them run wider.
2. **Agent chat:** the coloured code with Copy and the callouts are for the **Preview only** in this milestone; agent
   chat is redesigned with the agent box (M11ak) and the UI uplift (M11al).
3. **Links to a heading in another doc** (`guide.md#install`): open that file **in Preview at that heading**.

## Left to the builder

- The icons for Copy, the callout kinds and the narrow outline button (from the app's glyph set if it has them).
- The exact tokens' names and home (`ui_tokens` or `md_preview`), and the 36 em tuning to land near 70 characters.
- The front-matter reader (a small hand-written `key: value` reader, no YAML crate).
- How the code-colour job is tied to the version (as the File view's `accept_spans` is) so an older answer is dropped.
- GPUI details (hover groups, the overlay panel, `ListState` methods) checked against the pinned rev.
- Tests: `front_matter`, slugs, the callout marker, the badge rule, two-part pills, the current-heading rule and the
  linked-image parse are pure and unit-tested first; ui-shots scenes for the reader in light and dark: a long doc
  with the outline (wide) and the narrow button and panel, code with colours and Copied, each callout kind, a striped
  table, front matter, a badge row, the progress line part-way.

## Questions for you

1. **Width:** keep code blocks and tables inside the 70-character column (they wrap; a wide table squeezes), or let
   them grow wider than the prose up to the Preview's width?
2. **Chat:** should agent chat messages also get the new code blocks (colours, Copy) and callouts, or Preview only
   (AI's call: Preview only)?
3. **Links to another doc's heading** (`guide.md#install`): open it in Preview at that heading (AI's call), or keep
   opening the source at the top as today?

## What was built

All 12 plan tasks (0–11) on `m11ai-md-reader`; Tasks 0–10 each reviewed (4, 6 and 9 after one fix round), Task 11 (this hand-off) goes to the whole-branch review. Not merged.

- **Task 0, baselines:** the numbers the branch must not exceed (2,024 tests, clippy 14 / 27 and 16 / 29, the full
  ui-shots 205 / 205).
- **Task 1, the parser reads a linked image:** `[![alt](badge)](link)` is one span (`Span::LinkedImage`). Agent chat
  shows it as `[alt]`, linked; before, it fell apart into `[`, `[alt]` and `](…)`. That is chat's only visible change.
- **Task 2, the reader plan:** a new pure module, `src/md_reader.rs`. It splits off front matter, reads callouts,
  badge rows, heading slugs and the "On this page" outline, and works out the current entry, jumps and copied links.
  All of it is unit-tested.
- **Task 3, the column, type and spacing:** the Preview's code moved to `src/file_tab_preview.rs`. Prose sits in a
  480 pt centred column at 16 pt, with headings 30 / 24 / 20 / 17 / 16 / 15, 16 pt between blocks, and 32 pt above a
  heading and 12 under it. Code blocks and tables are at least the column wide and grow to the Preview's width. A wide
  table scrolls sideways inside itself. Chat keeps its sizes (`Sizes::CHAT`).
- **Task 4, code blocks:** coloured by the fence's language (a background job, cached by language, text and theme),
  with a header strip a shade darker than the code. The strip shows the language on the left and Copy on the right;
  after a click, Copy shows a tick and "Copied" for 1.5 s. A Mermaid block shown as code gets a `mermaid` header.
- **Task 5, callouts:** `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]` and `[!CAUTION]`. Each has a coloured bar,
  a wash behind it, an icon and a title. Any other `[!…]` stays a plain quote.
- **Task 6, looks like GitHub:**
  - Tables: roomier cells, and every second body row striped.
  - Front matter: a small two-column table (keys dim), "+ N more" past 20 keys, and a `front matter` code block when
    it isn't `key: value` lines.
  - Badges: drawn as pills from their alt text (two halves when the text has a `:`), never fetched.
- **Task 7, heading anchors:**
  - Hovering a heading shows a dim `#` to its left. A click on it copies the heading's link, which reads "Copied".
  - `#part` links jump inside the document.
  - `other.md#part` opens that file in Preview at the heading.
- **Task 8, "On this page" (wide):** a 200 pt column on the right once the Preview is 784 pt or wider. The current
  entry is marked with a bar and the accent colour and follows the scroll. A click on an entry jumps to it, and a
  long entry ends in "…" with a tooltip.
- **Task 9, narrow outline and progress:**
  - When the Preview is narrower, a list button sits at its top-right corner. It opens the outline as a floating
    panel; Esc, a click outside or an entry's click closes it.
  - A 2 pt accent line along the Preview's top shows how far you have read.
- **Merge of master (M11al) into the branch** (`2b9eb70`, between Tasks 8 and 9): the conflicts in the roadmap, the
  Progress Log and `tab_bar.rs` were resolved. The reader follows M11al's 28 / 22 pt padding from the token.
- **Task 10, reloads and big documents:**
  - A reload keeps your place even when front matter is added or removed.
  - Code colours come back from the cache at once, "Copied" clears, and the panel closes (and stays closed) when a
    reload leaves no outline.
  - One current-entry check runs per frame, however many wheel events arrive.
  - Code is coloured even if Preview was turned off while the syntaxes loaded.
  - The relayout and parse timings were measured (below), and `PREVIEW_MEASURE_ALL` went from 2,000 to 700.
- **Task 11, this hand-off:**
  - These docs.
  - The last literal sizes in the reader's drawing became `ui_tokens::reader` tokens, with chat's values kept in
    `Sizes::CHAT`. They are the callout icon gap, the code border, the list indent and number gap, the quote's
    padding and bar, the rule's margin and line, a table cell's padding, and the Mermaid diagram's corners and note.
  - The `#![allow(dead_code)]` lines in `md_reader.rs` and `ui_tokens::reader` are gone (nothing was dead).
- **The final review's fixes** (one wave, after the full ui-shots run and the whole-branch review):
  - "On this page"'s current entry is found by binary search over the outline's rows, which are worked out once per
    version (the plan's `outline_rows`), not by a scan that built a new list on every scroll frame.
  - Links and dim text inside a callout are held to the text floor on its wash, as its body text is (agent chat
    draws as before).
  - Front matter: a list of maps under a key (`- name: x`) reads as that key's items, not as new keys.
  - A theme change recolours the Preview's code blocks while Preview is off too.

### Calls made while building (AI's, for you to check)

**Sizes and layout**
- **Every size is a token** in `ui_tokens::reader`. Chat's sizes are kept exactly in `md_preview::Sizes::CHAT`. The
  Preview's padding is the terminal's (`TERMINAL_PADDING`, 28 / 22 since M11al).
- **The column is 480 pt, not 576.** A 70-character sentence measures 480 pt at 16 pt in the UI font. The design's
  "36 em" (576 pt) holds about 84 characters. The `file-tab-md-reader-column` scene checks the 70 characters.
- **The outline shows from 784 pt of Preview width**, worked out from the tokens: 480 + 2 × 28 + 48 + 200. The design
  guessed "about 880".
- **Inline code is drawn at the body size (16 pt), in the code font.** The design asked for 14 pt, but GPUI's text
  highlights cannot change the size inside a line. Chat's inline code was never smaller either.
- **Tables keep equal-width columns**, as the design says ("as today"). The side effect: a table with one long cell
  can fill the whole width.
- **Code blocks and tables are at least the column wide** and grow to the Preview's width. A table's columns are at
  least 96 pt. A table wider than the Preview scrolls sideways inside itself, while an up-and-down wheel over it
  scrolls the page.

**Tokens the plan named no number for**
- `COPY_GAP` 4: between Copy's icon and its word.
- `CODE_HEADER_SHADE` 0.05: how much darker the header strip is.
- `PILL_FILL` 0.08 and `PILL_VALUE_FILL` 0.16: a pill's two fills.
- `FRONT_GRID` (1 of 3): the front matter's key column.
- `TABLE_RADIUS` 4: a table's corners (M11ac's 4).
- `ANCHOR_TEXT` 12: the "Copied" beside a heading.
- `OUTLINE_PAD` 10: the outline's text inset.
- `OUTLINE_PANEL_ROWS` 12 and `OUTLINE_PANEL_GAP` 4: the narrow panel.
- Added in Task 11, each today's value:
  - `CALLOUT_GAP` 6 and `CODE_BORDER` 1;
  - `LIST_INDENT` 20 and `MARKER_GAP` 6;
  - `QUOTE_PAD` 12 and `QUOTE_BAR` 3;
  - `RULE_MARGIN` 4 and `RULE_LINE` 1;
  - `DIAGRAM_RADIUS` 6, and the Mermaid note's `NOTE_GAP` 4, `NOTE_TEXT` 12 and `NOTE_LINE` 1.4.

**Code blocks**
- **The header strip is a shade darker than the code's fill in every theme.** The design says darker, and that won
  over the plan's "toward the text", which made it lighter in dark themes. At 0.05 it is faint in a dark theme;
  item 2 of your check asks you to judge it.
- **The fence's first word is the language**, split at a space, `,` or `{`. The syntax colours are lifted to the text
  contrast floor on the code fill, so they can differ slightly from the File view's.
- **New icons** were drawn for Copy, Check, Outline and the callouts: Info, Bulb, Report, Alert and Octagon. The app's
  icon set had none of them.

**Parsing and links**
- **Front matter is the list's first row.** A reload keeps your place by row, shifted by any front-matter change. An
  empty front-matter block draws nothing.
- **A linked badge inside a sentence draws as a pill**, not as a link, and still follows its link.
- **The copied heading link** is the file's path from the repo's top plus `#slug` (e.g. `docs/SETUP.md#configure`).
  Outside a repo it is the `~` path, and outside your home the full path.
- **Anchors** match the slug exactly, then in lowercase. A bare `#` link does nothing.

**The outline**
- **The current entry is worked out after scrolling, never while drawing.** It runs from the scroll at the next frame
  (at most once a frame), right after a jump, and once after a reload, a remeasure or a resize. Where no window is at
  hand, a flag asks the Preview's next layout to arm that one-shot.
- **"Scrolls on its own" means the outline has its own scroll.** It does not move by itself to keep the current
  entry in view.
- **The outline's caption is the entries' size.** Every entry has a tooltip, because GPUI cannot tell which ones were
  cut off.
- **Esc closes the narrow panel first** and stops there. A second Esc does what it did before.

**Performance**
- **The parse stays on the UI thread, lazily at draw time, once per file version** (M11ac's way). The anchor jump and
  the code-colour job start at that same spot. Measured (debug, 1 MB): parse median 134.5 ms, plan 20.6 ms, so the
  plan adds about 15 %.
  - Plan-time call 15 said to move both off the UI thread if they passed 80 ms. The parse alone is already over that
    in debug, and it is M11ac's, unchanged.
  - The plan's share is the part this milestone adds, and it is well under the 50 % limit.
  - The cost: a hitch when a 1 MB document opens or reloads in Preview. The final review weighs it.
  - **For you to decide:** keep the Markdown parse (about 134 ms parse + 20 ms plan for a 1 MB file, debug) on the
    UI thread, or move the parse and the plan to the background in a follow-up.
- **`PREVIEW_MEASURE_ALL` went from 2,000 blocks to 700.** The reader's full relayout of a 2,000-block file took 2.76×
  master's in debug (table below). Call 15 lowers the limit by that ratio, rounded down to a hundred.
  - Documents of 701–2,000 blocks now get the estimated scrollbar instead of an exact one.
  - Where the time goes was not profiled.
  - The 2.76× was measured in debug ui-shots builds, which add test-only probes to every Preview row (master has
    none), so it overstates the app's cost. The limit 700 is the safe side.
- **All timings are debug builds**, because `--release` is not run here (your rule). They stand in for the design's
  release-build numbers.

**Rulings during the build**
- **The Preview's code moved to `src/file_tab_preview.rs`**, a `#[path]` child module of `file_tab.rs` (which is
  about 11,000 lines). `file_tab.rs` keeps the fields, `preview_shown` and the switch.
- **Task 4's Mermaid header** is asserted on whatever code-shown state the scene reaches (waiting, too many or
  failed): the plain scenes have no WebKit, so they never draw a diagram.
- **Task 10's RED** included the front-matter shift test failing first (a reload that adds or removes front matter).
- **The "quote in a list item" test became "quote inside a quote"** (a callout marker there stays a plain quote).
  The parser never nests a quote in a list item.

**Process and testing**
- **Your calls during the build:**
  - M11ai **paused after Task 8** so M11al had the machine, and resumed after M11al merged.
  - From Task 9 on, each task ran `scripts/ui-shots.sh --changed master` only. The full suite runs once before the
    final review and again after its fix wave.
- **Test-only behaviour (ui-shots builds only):** frozen test tabs record a followed web link instead of opening a
  browser. The anchors scene runs `git init` in its own folder, so the copied link is repo-relative.
- **Docs were written once, in Task 11**, not per task.

### Known small gaps (things you might notice)

**The outline**
- **The end of a big document** (over 700 blocks): at the very bottom the last short section is not marked. The
  current entry is the last heading you scrolled past, because the "at the end" rule needs every row measured.
- **A long jump in a big document** (over 700 blocks): one wheel step longer than about 400 pt can leave the wrong
  entry marked until your next scroll (the check reads the rows measured before that step).
- **A table inside a callout:** its striped rows' links and dim text are held to the floor on the page's stripe, not
  on the stripe over the callout's tint.
- **Near-end entries:** clicking an outline entry near the end can light it for one frame, then the "at the end"
  rule marks the last heading instead.
- **The narrow panel** does not scroll to the current entry when it opens. With more than 12 entries, the current one
  can be below the panel's visible rows.
- **The narrow outline button** has no fill, so a wide code block or table at the very top can sit under it.

**Drawing**
- **"Copied" beside a heading** can spill past the Preview's left edge in a narrow layout.
- **A very long badge pill** (a long value) runs past the column. Pills wrap between each other, not inside one.
- **Inline code** is 16 pt, not the design's 14 (see the calls above).
- **A wide table with one long cell** fills the whole width (equal columns).
- **The code header strip** is faint in dark themes (item 2 of your check).
- **A wide table scrolling sideways** is not proven by a scene: the 500 pt scene does not show a table wider than
  the Preview scrolling inside itself.

**Themes and links**
- **A heading link into a Markdown file too big to preview** (over 1 MB) turns that file's Preview setting on and
  saves it, which a hand click would refuse. The source still shows, since the file does not fit.
- **The `#` copy right after a file opens**, before the repo is known, copies the `~` form.

**Parsing**
- **Anchors:** `%+1` is accepted as a percent escape (the hex reader takes a leading `+`), so such a part decodes to
  a control character instead of staying as written.
- **Badges:** a badge source like `//img.shields.io/…` (no `https:`), or a host followed straight by `?`, is not
  seen as a badge.
- **Slugs:** an accented heading written with combining marks can get a slug that differs from GitHub's.

**Performance**
- **A 1 MB document in Preview** hitches on open and reload: about 155 ms in debug, mostly M11ac's parse.
- **Documents of 701–2,000 blocks** get the estimated scrollbar.

## Measured headlessly

All timings are debug builds on this Mac. `--release` is not run here.

**Full relayout of a 2,000-block mixed Markdown file in Preview** (remeasure plus one draw, 1,200 × 700, Mocha; 2
warm-up runs, then 5 timed runs). The probe was temporary and not committed. Master ran from a temporary detached
worktree that was removed afterwards.

| Build | Runs (ms) | Median |
|---|---|---|
| master `884e63a` (M11ac / M11ae Preview) | 120.40, 120.55, 120.82, 121.64, 121.70 | **120.8 ms** |
| master, second run | 120.26, 120.50, 120.89, 122.58, 122.93 | 120.9 ms |
| branch (Task 10) | 331.93, 332.38, 333.26, 333.97, 335.09 | **333.3 ms** |
| branch, second run (forced rebuild) | 335.95, 336.71, 339.33, 339.66, 361.73 | 339.3 ms |

The ratio, branch over master, is **2.76** (2.81 on the second run). That is over 1.15, so `PREVIEW_MEASURE_ALL`
became 2,000 / 2.76 = 724, rounded down to **700**.

**Parse + plan of 1 MB** (`md_reader::tests::read_plans_a_megabyte_quickly`, 38,129 blocks, 5 runs):

| | Parse | Plan | Total | Plan / parse |
|---|---|---|---|---|
| median | **134.5 ms** | **20.6 ms** | 155.1 ms | 15 % |
| range | 125.2–171.1 ms | 19.0–30.7 ms | 144.2–195.7 ms | 14–22 % |

The plan is well under the 50 % limit. The total is over the 80 ms line, and the parse alone (M11ac's) is too. Both
stay on the UI thread (see the calls above).

Idle CPU and the feel of scrolling need a real window. They are items 14 and 15 below.

## Final numbers (this Mac, debug build, 2026-11-18)

- `cargo test --bin fuxx`: 2,076 passed, 0 failed, 1 ignored (baseline 2,024), after the final review's fixes.
- clippy: 14 (bin) / 27 (test) by default, and 16 / 29 with `ui-shots`. These equal the baselines.
- `cargo fmt --check`: clean.
- `scripts/ui-shots.sh --changed master` (Task 11's gate): 226 passed, 0 failed, 150.2 s (every scene is selected, since shared sources changed); after the final review's fixes: 226 passed, 0 failed, 146.0 s. The reference fingerprints are stale on this Mac (339 "look different"); they are reported, not failures, and were not updated.
- `scripts/ui-shots.sh` (full): 226 passed, 0 failed, 151.3 s before the final review's fixes; 226 passed, 0 failed,
  151.4 s after them.

## On-screen check (yours, after the build)

Run `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` from the worktree. This is a debug build only: **do not add
`--release`**.

Make a made-up doc in a scratch repo (`git init ~/code/my-app`), at `~/code/my-app/docs/SETUP.md`. Give it:
- a "Setup guide" README with front matter (`title`, `description`, `tags`, `owner`);
- a row of badges (`[![build: passing](https://img.shields.io/badge/build-passing-green)](https://example.com/ci)`
  and two more);
- all five callouts;
- code blocks in `sh`, `ts` and `rust`;
- a wide 6-column table with 6 or more rows;
- 12 or more `##` sections, one with a `###`;
- a link `[see Configure](#configure)`, and a link `[the other guide](other.md#middle)` to a second file
  `docs/other.md` with `## Middle` between plenty of paragraphs.

Open `SETUP.md` in a file tab and turn Preview on.

1. **Column and type:** in a wide window the text sits in a centred column of about 70 characters, at 16 pt. There is
   clearly more room above each heading than under it. In a narrow pane the column fills the width, inside the
   terminal's padding.
2. **Code:**
   - The three blocks are coloured. Each shows its language at the top left, and Copy at the top right, which gets
     brighter under the pointer.
   - Click Copy: a tick and "Copied" show for about 1.5 s, and pasting gives the block's text.
   - In a dark theme, is the header strip visibly a shade darker than the code? If it is too faint, say so (one token
     to change).
3. **Callouts:** Note, Tip, Important, Warning and Caution each have a coloured bar on the left, a faint wash, an icon
   and a bold title. A `> [!FOO]` quote stays a plain quote.
4. **Table:**
   - The header is filled, and every second body row is lightly striped.
   - The wide table scrolls sideways (shift-wheel or a sideways swipe), while an up-and-down wheel over it scrolls the
     page.
5. **Front matter:** a small two-column table at the very top, keys dim. With more than 20 keys, the last row reads
   "+ N more".
6. **Badges:** the badge row shows pills (label, plus a darker value half), with no network use. Clicking a linked
   pill opens its link in the browser. A badge inside a sentence shows as a pill too.
7. **Heading anchors:**
   - Hover a heading: a dim `#` shows to its left. Click it: "Copied", and the clipboard holds
     `docs/SETUP.md#configure`.
   - Click `see Configure`: the view jumps there, with the heading just under the top.
8. **Cross-doc link:** click `the other guide`: `other.md` opens in a tab, in Preview, at "Middle".
9. **Wide outline:** with the Preview about 800 pt or wider:
   - "On this page" shows on the right, and the current entry (bar and accent colour) follows as you scroll.
   - Click an entry: it jumps there.
   - A long entry ends in "…" and its tooltip shows the full text.
   - At the very bottom, the last section is current.
10. **Narrow outline:** narrow the pane below about 780 pt:
    - The outline hides and a list button sits at the top-right corner.
    - Click it: the outline floats under it. Click an entry: it jumps and closes.
    - Open it again: Esc closes it, and a second Esc leaves Preview on. A click outside also closes it.
11. **Progress line:** a thin accent line along the Preview's top grows as you scroll and spans the width at the end.
    A short doc that fits shows none.
12. **Live reload:** scroll to a middle section. In another terminal, add three lines of front matter at the top of
    the file and save:
    - the view stays at the same section;
    - the code colours stay;
    - the outline still marks the same entry.

    Remove the front matter and save: the view is still in place.
13. **Light and dark:** switch between a light and a dark theme with Preview on. Everything stays readable: code
    colours, callout titles, pills, striped rows, the outline and the panel.
14. **Idle CPU (the M11p anti-goal), by hand:** with the long doc shown in Preview and nothing moving:
    - run `top -l 6 -s 5 -pid <fuxx pid>` in another terminal, and take the median CPU of the samples;
    - do the same with `master` built the same way (debug, `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`).

    They should match within noise.
15. **Scrolling feel on a long doc:**
    - Paste the sections a few times, to about 600 blocks: scrolling stays smooth, and the outline keeps up without
      flicker.
    - Then go past 700 blocks: the scrollbar becomes an estimate, and scrolling should still feel the same.
    - A 1 MB doc may pause briefly when it opens (known; see Known small gaps).

## Related

[[Proposal - File View Upgrades]] · [[78 - Milestone 11ac - File Tab Polish]] ·
[[80 - Milestone 11ae - File Tab Chores]] · [[77 - Milestone 11p - Chrome Caching]] ·
[[82 - Milestone 11af - Live Changes]] · [[84 - Milestone 11al - UI Uplift 3]] · [[12 - Scope]] · [[40 - Roadmap]]
