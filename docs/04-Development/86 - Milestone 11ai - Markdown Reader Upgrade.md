# Milestone 11ai — Markdown Reader Upgrade

**Status:** 📝 design approved 2026-11-17 on your go-ahead ("we already design it, layout is also built before. just go ahead"); its three open questions settled as AI's calls (below). **Planned 2026-11-18:** [[M11ai Plan - Markdown Reader Upgrade]] (Tasks 0–11), building on branch `m11ai-md-reader`. Built **after M11ah** (code view
upgrade), in sequence: both live in the file tab.

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
  out from the tokens).
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

## Related

[[Proposal - File View Upgrades]] · [[78 - Milestone 11ac - File Tab Polish]] ·
[[80 - Milestone 11ae - File Tab Chores]] · [[77 - Milestone 11p - Chrome Caching]] ·
[[82 - Milestone 11af - Live Changes]] · [[84 - Milestone 11al - UI Uplift 3]] · [[12 - Scope]] · [[40 - Roadmap]]
