---
tags: [development, plan, milestone]
milestone: M11ai
status: built
updated: 2026-11-18
---

# M11ai Plan — Markdown reader upgrade

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[86 - Milestone 11ai - Markdown Reader Upgrade]] |
| **Status** | 🟨 built on branch `m11ai-md-reader`, waiting for the on-screen check, not merged (planned 2026-11-18 from master `575d010`; master with M11al merged in at `2b9eb70`) |
| **Tasks done** | 12 / 12 |
| **Baselines** | on commit 6c685ce: `cargo test --bin fuxx` 2024 passed, 1 ignored; clippy default 14 (bin) / 27 (test), `ui-shots` 16 / 29; `cargo fmt --check` clean; full `scripts/ui-shots.sh`: 205 passed, 0 failed, 167.2 s (no failures, no flake seen) |
| **▶ Pick up here** | Your on-screen check ([[86 - Milestone 11ai - Markdown Reader Upgrade]], "On-screen check") |
| **Decisions used** | Yours: "we already design it, layout is also built before. just go ahead" (design and its three open questions as AI's calls). The design's "AI's calls" and "Calls on the open questions". Plan-time calls below (AI's). |
| **Needs your eyes** | 👀 the on-screen check in note 86, after Task 11 |

**Goal:** The file tab's Markdown Preview becomes a comfortable reader: a centred ~70-character column, coloured code
blocks with their language and Copy, GitHub callouts, heading anchors, an "On this page" outline that follows you, a
progress line, striped tables, front matter as a small table and badges as pills — Preview only, offline, theme-true.

**Architecture:** A new pure module `src/md_reader.rs` splits front matter off the text, parses the rest with the
shared `agent_markdown::parse` (which learns one thing: a linked image) and builds a per-version `ReaderPlan`
(rows, headings and slugs, callouts, badge rows, the outline). `md_preview.rs` draws blocks with a `Sizes` set
(chat's today's sizes, the reader's tokens) and, only when `Draw::reader` is set, the reader's looks. The tab's
Preview code moves out of `file_tab.rs` into `src/file_tab_preview.rs` (a child module, `#[path]`, as
`agent_box_menus.rs` is), which owns the reader state: code colours (a background job, cached), Copied, the
outline's current entry (worked out on scroll, never while drawing), the narrow outline panel and anchor jumps.

**Tech Stack:** Rust 2024, GPUI (pinned rev `1d2a4b3f`, unchanged; `ListState::bounds_for_item`,
`logical_scroll_top`, `scroll_to`, `is_scrolled_to_end` and `set_scroll_handler` read from its `list.rs` at that rev
— the scroll handler runs inside the list's `RefCell` borrow, so it must not read the list), `syntect` + `two-face`
through `src/highlight.rs` (`SyntaxSet::find_syntax_by_token`, checked with Context7 2026-11-17). **No new crates.**

**Spec:** [[86 - Milestone 11ai - Markdown Reader Upgrade]] (binding, with its "Calls on the open questions").
Read [[85 - Milestone 11ah - Code View Upgrade]] for the file tab as it is now.

## Plan-time calls (AI's, made while reading the code)

1. **Chat keeps today's sizes.** `md_preview::block` is shared with agent chat (`column`), so the reader's bigger
   type cannot be a change of today's constants: `md_preview::Sizes` holds them, `Sizes::CHAT` is today's numbers
   exactly, `Sizes::READER` is built from the tokens, and `Draw` carries one.
2. **Tokens live in `ui_tokens::reader`** (beside M11ah's `ui_tokens::code`). The Preview's side and top padding is
   `ui_tokens::size::TERMINAL_PADDING` itself (16 / 12 today; M11al moves it).
3. **The Preview code moves to `src/file_tab_preview.rs`** (`#[path = "file_tab_preview.rs"] mod preview;` in
   `file_tab.rs`, `use super::*`): `preview_body`, `mermaid_shown`, `wants_mermaid`, `follow_link`, the Preview's
   scrollbar handlers and everything new. `file_tab.rs` keeps the fields and `preview_shown`.
4. **Rows and blocks.** List row 0 is the front matter when there is one; every other row is a block
   (`ReaderPlan::row_kind`). The Mermaid plan and the code colours stay indexed by block. A reload keeps the place
   by row, shifted by the change in front-matter rows (so adding front matter does not move you a block).
5. **The parse stays where it is** (lazily in `preview_body`, once per version — M11ac's precedent, as is its
   `scroll_to(top)` on a reload). A pending anchor (a cross-doc link) is applied at that same spot. The code-colour
   job starts there too (precedent: Mermaid's queue in `mermaid_shown`).
6. **The linked image is a new `Span::LinkedImage { alt, src, url }`.** In chat it draws as `[alt]`, linked —
   today the same text falls apart into `[` `[alt]` `](…)`. That is chat's only visible change.
7. **Wide blocks (spec "Calls" 1):** code blocks and tables are at least the prose column wide and grow with their
   content up to the reader area's width, centred on the column. A table's columns are at least
   `reader::TABLE_COL_MIN` (96 pt); a table wider than the area scrolls sideways inside itself. Code wraps at the
   area's width, as today.
8. **The fence's first word** (split at whitespace, `,` or `{`) is the language: shown in the header and used for
   `find_syntax_by_token`. Syntax colours are pushed to the text floor on the code fill (`TEXT_MIN`).
9. **Icons:** `tab_bar::Glyph` gains `Copy`, `Check`, `Outline` (three lines), and the callouts' `Info`, `Bulb`,
   `Report` (a speech box with `!`), `Alert` (a triangle with `!`) and `Octagon` (an octagon with `!`, Caution's;
   the existing `Stop` is a plain cross). "Copied" shows `Check` and the word.
10. **The copied heading link** is the file's path from its repo's top folder (the Files tree's root, the tab's
    `GitPlace.rel`) plus `#slug`; outside a repo, its `~`-relative path plus `#slug`.
11. **Anchors:** `#part` matches a slug exactly, else its lowercase; percent-decoded. `LinkTarget` gains
    `Here(String)` (a `#part` of this file), and `File` carries the part: `File(PathBuf, Option<String>)`.
    `FileTabEvent::OpenFile` carries it too; the workspace opens a Markdown target with a part in Preview.
12. **The outline's current entry** is worked out by `FileTab::track_heading`, called (a) from the list's scroll
    handler through `window.on_next_frame` (out of the list's borrow; before that frame's layout, from the new
    scroll top and the measured rows), (b) right after a jump (the jumped-to entry), (c) once, at the start of the
    frame after a remeasure, a reload or a width change (a one-shot `on_next_frame` armed when the remeasure is
    asked; never re-armed from itself). No entry is current until the first is reached.
13. **The outline shows** when the Preview's width (measured in its canvas's prepaint into an
    `Rc<Cell<Option<bool>>>`, one deferred notify when it flips — as `fits_side` does for side by side) is at least
    `reader::MEASURE + 2 × TERMINAL_PADDING.0 + reader::OUTLINE_GAP + reader::OUTLINE_W`.
14. **Front matter:** a first line of exactly `---` (trailing spaces allowed; a leading U+FEFF skipped), closed by a
    line of `---` or `...`. One matching pair of quotes round a value is dropped; `|` / `>` block values and nested
    keys join their indented lines with `, ` for keys and a space for text. An empty block is split off and draws
    nothing.
15. **Timing without `--release`:** the 2,000-block relayout is measured in a debug ui-shots probe on this branch
    and on master on the same file; if the branch is over 1.15 × master, `PREVIEW_MEASURE_ALL` drops by that ratio
    (rounded down to a hundred). Parse + plan of a 1 MB file is timed in a debug test: if the plan adds more than
    50 % to the parse, or the two exceed 80 ms, both move to the background executor with the read.

## Global Constraints

- Worktree `.worktrees/m11ai-md-reader`, branch `m11ai-md-reader` only; never the main checkout or another worktree.
  Commit per task; no merge, no push.
- **Gate per task:** `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features
  ui-shots && cargo fmt --check` (no new warnings over Task 0's baselines), then `scripts/ui-shots.sh --changed
  master`, and the full `scripts/ui-shots.sh` before the commit of any task that touches drawing. A failing scene is
  re-run alone once before it counts. Reference fingerprints in `tests/ui-shots-ref` are stale — **never
  `--update-refs`**.
- **M11p:** nothing visible changes inside any `render`; no always-on timer (one-shots are fine: Copied's 1.5 s).
  The outline is never tracked while drawing.
- **Offline:** nothing is fetched or read for an image or badge; a badge is drawn from its alt text.
- **Scope (spec non-goals):** no editing, no prose selection, no find in Preview (⌘F still turns it off), no HTML,
  footnotes, strikethrough, reference links or math, no zoom, no reader looks in chat, none of M11ah's code-view
  features in Preview, no new keyboard shortcuts (Esc only closes the narrow panel), no `+++` front matter.
- **Chat unchanged** except plan-time call 6: `md_preview::column` draws with `Sizes::CHAT` and `reader: None`.
- **Every size is a token** in `ui_tokens::reader` (values below); **every colour from the theme**, text at
  `TEXT_MIN` (4.5:1) on what it is drawn on, bars and icons at `MARK_MIN` (3:1). Light and dark shots for each look.
- Token values: `BODY` 16, `BODY_LINE` 1.65, `HEADINGS` [30, 24, 20, 17, 16, 15] (H6 dim), `HEADING_LINE` 1.3,
  `BLOCK_GAP` 16, `HEADING_ABOVE` 32, `HEADING_BELOW` 12, `ITEM_GAP` 6, `CODE` 14, `CODE_LINE` 1.5, `RULE_BELOW` 8
  (H1/H2 underline under the text), `MEASURE` 576 (36 em of 16), `CODE_HEADER` 28, `CODE_HEADER_TEXT` 12,
  `CODE_PAD` (12, 10), `CODE_RADIUS` 6, `CALLOUT_BAR` 3, `CALLOUT_PAD` 12, `CALLOUT_RADIUS` 6,
  `CALLOUT_WASH` 0.08, `TABLE_CELL` (13, 7), `TABLE_COL_MIN` 96, `STRIPE` 0.035, `FRONT_TEXT` 13, `FRONT_ROWS` 20,
  `PILL_H` 20, `PILL_TEXT` 12, `PILL_PAD` 7, `PILL_GAP` 6, `OUTLINE_W` 200, `OUTLINE_GAP` 48, `OUTLINE_TEXT` 13,
  `OUTLINE_ROW` 26, `OUTLINE_INDENT` 12, `OUTLINE_BAR` 2, `PROGRESS` 2, `JUMP_ROOM` 16, `REACH` 0.25,
  `COPIED` 1.5 s, `ANCHOR_GAP` 6 (the `#` left of a heading).
- Exact copy: `Copy`, `Copied`, `On this page`, `Note`, `Tip`, `Important`, `Warning`, `Caution`,
  `front matter`, `+ N more`, `badge`.
- PUBLIC repo: made-up example documents only; no private names, no absolute home paths, no time-zone names. Grep
  each commit's added lines for `/Users/` before it lands.
- Shell safety: no backticks or `$(...)` inside double-quoted shell strings; single quotes for grep patterns; never
  run `fuxx` by name. The app, if ever run, only as `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug).
- Commits stage files by path; no Co-Authored-By or AI attribution lines.
- GUI looks are proven by scene probes (bounds, pixels, state); agents can't see the window — say so in reports.

## Review Focus

1. **A file that starts with `---` but is not front matter** (a rule, then a setext heading; no closing line; a
   closing line far down after prose): with no closing line the text reads exactly as today; with one, everything
   between is front matter (raw if not `key: value`). Pinned in Task 2 (`front_matter_needs_its_closing_line`,
   `a_rule_then_text_is_not_front_matter`).
2. **Headings whose slug is odd** — repeats, non-ASCII (`## Café`), emoji (`## 🚀 Launch`), only punctuation
   (`## !!!`), inline code and links in the title: GitHub's slugs, repeats `-1`, `-2`, an empty slug still unique.
   Pinned in Task 2 (`slugs_follow_github`, `repeated_and_empty_slugs_stay_unique`).
3. **A fence with more than a language** (`rust,ignore`, `{.python}`, `TS`, `sh title="x"`, an unknown `foo`): the
   first word shows and picks the syntax (case-insensitive); unknown is plain code colour, never an error. Pinned in
   Task 4 (`the_fence_s_first_word_is_the_language`, `an_unknown_language_is_plain`).
4. **Reload with the reader in a state** (scrolled; front matter added or removed; a code block shown "Copied";
   the narrow outline open): the place is kept by row, shifted by the front matter's change; unchanged code keeps
   its colours at once; Copied goes; the panel stays open on the new outline (closed if it has under 2 entries).
   Pinned in Task 10 (scene `file-tab-md-reader-reload`).
5. **Copy on a code block with tabs and trailing spaces**: the clipboard holds the block exactly as in the file
   (tabs kept, no widening), while the drawing widens tabs. Pinned in Task 4 (`code_runs_widen_tabs_and_keep_colours`,
   scene `file-tab-md-reader-code` reads the clipboard).

---

### Task 0: Baselines

**Files:**
- Modify: this plan's "At a glance"

- [x] **Step 1:** On the clean branch run the gate; write the test count (passed, ignored), clippy warning counts
  (bin / test, default and `ui-shots`), fmt state and the full `scripts/ui-shots.sh` result (passed / failed, time,
  any known flake) into "At a glance".
- [x] **Step 2:** Commit `M11ai Task 0: baselines` (`git add "docs/04-Development/Plans/M11ai Plan - Markdown Reader Upgrade.md"`).

### Task 1: The parser learns a linked image

**Files:**
- Modify: `src/agent_markdown.rs` (`Span::LinkedImage`, `spans()` tries it at `[` before `link`), `src/md_preview.rs`
  (`inline` draws it as `[alt]` linked)

**Interfaces:**
- Produces: `Span::LinkedImage { alt: String, src: String, url: String }` — `[![alt](src)](url)`; the scan stays
  linear (stops at a line break or a second `[` outside the image, as `link` does).

- [x] **Step 1: Write the failing tests** in `agent_markdown.rs`:
  - `a_linked_image_is_one_span`: `[![build: passing](https://img.shields.io/b.svg)](https://example.com/ci) after`
    → `[LinkedImage { alt: "build: passing", src: "https://img.shields.io/b.svg", url: "https://example.com/ci" },
    Text(" after")]`; `[![a](s)]` (no link) → `[Text("["), Image { alt: "a", url: "s" }, Text("]")]`;
    `[![a](s)](` (unclosed) keeps the image; plain `[x](y)` and `![x](y)` parse as before.
  - `md_preview.rs` `a_linked_image_reads_as_its_alt_linked`: `inline(&[LinkedImage{alt:"ci",src:"s",url:"u"}])` →
    text `[ci]`, looks `[(0..4, Link(0))]`, links `[(0..4, "u")]`.
- [x] **Step 2: Run** `cargo test --bin fuxx linked_image` — FAIL (no variant). **Step 3: Implement.**
  **Step 4: Run** — PASS; `a_megabyte_of_unclosed_markers_parses_quickly` still passes; the gate.
- [x] **Step 5: Commit** `M11ai Task 1: the parser reads a linked image`.

### Task 2: The reader plan (`src/md_reader.rs`, pure)

**Files:**
- Create: `src/md_reader.rs` (declared in `src/main.rs`)

**Interfaces:**
- Consumes: Task 1's `Span::LinkedImage`.
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq)] pub struct FrontTable { pub rows: Vec<(String, String)>, pub more: usize }
  #[derive(Clone, Debug, PartialEq)] pub enum Front { Table(FrontTable), Raw(String) }
  /// The front matter and the text after its closing line (`None`: the whole text).
  pub fn front_matter(text: &str) -> (Option<Front>, &str);
  pub fn heading_text(spans: &[Span]) -> String;          // text, code, bold, italic, link text; no image alt
  pub fn slug(text: &str) -> String;                       // GitHub's
  pub fn slugs(texts: &[String]) -> Vec<String>;           // repeats get -1, -2…
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Callout { Note, Tip, Important, Warning, Caution }
  impl Callout { pub fn title(self) -> &'static str; pub const ALL: [Callout; 5]; }
  /// A top-level quote's blocks as a callout: its kind and its body (the marker line taken out).
  pub fn callout(inner: &[Md]) -> Option<(Callout, Vec<Md>)>;
  pub fn is_badge_src(src: &str) -> bool;
  #[derive(Clone, Debug, PartialEq)] pub struct Badge { pub label: String, pub value: Option<String>, pub url: Option<String> }
  /// `"license: MIT"` → ("license", Some("MIT")); `""` → ("badge", None) (drawn dim).
  pub fn pill(alt: &str) -> (String, Option<String>);
  /// A paragraph made only of badges and spaces → its badges.
  pub fn badge_row(spans: &[Span]) -> Option<Vec<Badge>>;
  #[derive(Clone, Debug, PartialEq)] pub enum BlockLook { Plain, Callout(Callout, Vec<Md>), Badges(Vec<Badge>) }
  #[derive(Clone, Debug, PartialEq)] pub struct Heading { pub block: usize, pub level: u8, pub text: String, pub slug: String }
  #[derive(Clone, Debug, PartialEq)] pub struct Entry { pub heading: usize, pub indent: bool }   // index into headings
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum RowKind { Front, Block(usize) }
  #[derive(Clone, Debug, Default, PartialEq)]
  pub struct ReaderPlan { pub front: Option<Front>, pub looks: Vec<BlockLook>, pub headings: Vec<Heading>, pub outline: Vec<Entry> }
  impl ReaderPlan {
      pub fn of(front: Option<Front>, blocks: &[Md]) -> ReaderPlan;
      pub fn rows(&self) -> usize;                                // blocks + 1 with front matter
      pub fn row_kind(&self, row: usize) -> RowKind;
      pub fn row_of_block(&self, block: usize) -> usize;
      pub fn front_rows(&self) -> usize;                          // 0 or 1
      pub fn anchor_row(&self, anchor: &str) -> Option<usize>;    // plan-time call 11
  }
  /// Split, parse and plan a file's text (once per version).
  pub fn read(text: &str) -> (Vec<Md>, ReaderPlan);
  /// Which outline entry is current (plan-time call 12): `rows[i]` is entry i's list row.
  pub fn current_entry(rows: &[usize], at_end: bool, reached: impl Fn(usize) -> bool, in_view: impl Fn(usize) -> bool) -> Option<usize>;
  /// How far into a heading's row a jump scrolls, so its text is `JUMP_ROOM` under the top.
  pub fn jump_offset(gap_above: f32) -> f32;                       // (gap_above - JUMP_ROOM).max(0)
  /// The copied heading link (plan-time call 10).
  pub fn heading_link(rel: Option<&Path>, path: &Path, home: Option<&Path>, slug: &str) -> String;
  /// The fence's language (plan-time call 8).
  pub fn fence_lang(lang: Option<&str>) -> Option<&str>;
  ```
- Rules the tests pin: callout markers exactly `[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`, any
  case, spaces round allowed, alone on the quote's first line; only a top-level quote (`ReaderPlan::of` looks at
  `blocks` only, not inside lists or quotes). Badge sources: host `img.shields.io`, `badgen.net`, `badge.fury.io`,
  `codecov.io`, or a path ending `badge.svg` (query ignored); a paragraph of two or more images (linked or not) and
  spaces is a badge row whatever the sources. Outline: levels 1–3, H3 `indent`; a file whose only H1 is its first
  heading leaves it out; fewer than 2 entries → empty. `FRONT_ROWS` (20) rows, then `more`.

- [x] **Step 1: Write the failing tests** (in `md_reader.rs`):
  - `front_matter_reads_key_value_pairs`: `"---\ntitle: Setup guide\nstatus: draft\ntags: [docs, setup]\n---\n# Hi"`
    → `Table { rows: [("title","Setup guide"),("status","draft"),("tags","docs, setup")], more: 0 }`, body `"# Hi"`;
    a `- item` list under `tags:` → `"item, item"`; `title: "Quoted"` → `Quoted`; `...` closes too; 25 keys → 20
    rows, `more: 5`.
  - `front_matter_needs_its_closing_line`: `"---\ntitle: x\n\nSome text"` → `(None, whole)`; `"---\n---\nA"` → empty
    table, body `"A"`; a leading `\u{feff}` is skipped.
  - `a_rule_then_text_is_not_front_matter`: `"Intro\n\n---\n\nMore"` → `(None, whole)`.
  - `front_matter_that_is_not_pairs_is_raw`: `"---\njust some words\n---\n"` → `Raw("just some words")`.
  - `slugs_follow_github`: `Getting Started!` → `getting-started`; `Café au lait` → `café-au-lait`; `🚀 Launch` →
    `-launch`; `API_v2 - notes` → `api_v2---notes`; `heading_text` of `[Code("fn"), Text(" & "), Link{"docs",…}]`
    → `fn & docs` → slug `fn--docs`.
  - `repeated_and_empty_slugs_stay_unique`: `["Intro","Intro","!!!","!!!","Intro"]` → `["intro","intro-1","","-1",
    "intro-2"]`.
  - `callouts_by_marker`: each of the five markers (one lower case, one with spaces) → its kind, body without the
    marker line; `[!FOO]` → None; marker with no body → empty body; `[!NOTE] text` on one line → None (marker must be
    alone); a quote nested in a list item stays `Plain`.
  - `badges_by_source_or_by_company`: `is_badge_src` true for the four hosts and `…/ci/badge.svg?branch=main`, false
    for `logo.png`; a paragraph `[LinkedImage(shields), Text(" "), Image(shields)]` → two badges (first with url);
    `[Image("a.png"), Text(" "), Image("b.png")]` → two badges; `[Image("a.png")]` alone → None; image plus words →
    None.
  - `pills_split_at_colon_space`: `license: MIT` → (`license`, `MIT`); `build` → (`build`, None); `a:b` → (`a:b`,
    None); `""` → (`badge`, None).
  - `the_plan_maps_rows_headings_and_outline`: a doc with front matter, `# Title`, `## A`, `### B`, `## A`, a callout
    and a badge row → `rows()` = blocks + 1, `row_kind(0)` = Front, `row_of_block(0)` = 1, slugs `title`, `a`, `b`,
    `a-1`, outline leaves the title out (3 entries, B indented), `anchor_row("A-1")` = the second A's row,
    `anchor_row("nope")` = None; a doc with one H2 → empty outline.
  - `the_current_entry_is_the_last_one_reached`: rows `[2, 10, 30]`; reached = row ≤ 12 → `Some(1)`; none reached →
    None; `at_end` with in_view = row ≥ 25 → `Some(2)` even though 30 is not reached.
  - `a_jump_leaves_room_above_the_heading`: `jump_offset(32.0)` = 16; `jump_offset(12.0)` = 0.
  - `heading_links_are_repo_relative`: (`Some("docs/guide.md")`, …, `install`) → `docs/guide.md#install`; outside a
    repo with home `/h` and path `/h/notes/a.md` → `~/notes/a.md#install`.
  - `fence_lang_is_the_first_word`: `rust,ignore` → `rust`; `{.python}` → `python`; `sh title="x"` → `sh`; `""` /
    None → None.
- [x] **Step 2: Run** `cargo test --bin fuxx md_reader` — FAIL. **Step 3: Implement** (`#![allow(dead_code)]` at the
  module top until Task 3 uses it; Task 11 removes any that remains).
- [x] **Step 4: Run** — PASS; add `read_plans_a_megabyte_quickly` (a 1 MB mixed doc: plan time ≤ parse time × 1.5,
  printed; plan-time call 15); the gate.
- [x] **Step 5: Commit** `M11ai Task 2: the reader plan`.

### Task 3: The reader's column, type and spacing

**Files:**
- Modify: `src/ui_tokens.rs` (`pub mod reader`, the values in Global Constraints), `src/md_preview.rs` (`Sizes`,
  `Draw::sizes`, `Draw::reader`, `reader_row`, wide blocks, front matter's row placeholder), `src/agent_tab.rs`
  (its `Draw` gets `sizes: Sizes::CHAT, reader: None`)
- Create: `src/file_tab_preview.rs` (plan-time call 3: the Preview code moved from `file_tab.rs`, unchanged in
  behaviour, then the reader's parse), `src/ui_shots/file_tab_md_reader.rs` (a new area: `FILE`, `SCENES`, in
  `registry.rs` `AREAS`/order and `mod.rs`; sources `src/file_tab_preview.rs`, `src/md_preview.rs`,
  `src/md_reader.rs`, `src/agent_markdown.rs`, `src/highlight.rs`, `src/ui_tokens.rs`)
- Modify: `src/file_tab.rs` (`parsed: Option<(u64, Rc<ReaderDoc>)>`; the moved code gone; `mod preview`)

**Interfaces:**
- Consumes: Task 2's `read`, `ReaderPlan`, `RowKind`.
- Produces:
  ```rust
  // md_preview.rs
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub struct Sizes { pub body: f32, pub line: f32, pub headings: [f32; 6], pub heading_line: f32, pub code: f32,
                     pub code_line: f32, pub block_gap: f32, pub item_gap: f32, pub heading_above: f32,
                     pub heading_below: f32, pub rule_below: f32 }
  impl Sizes { pub const CHAT: Sizes; /* 14, 1.5, [24,20,17,15,14,13], 1.3, 13, 1.45, 12, 4, 12, 12, 6 */ pub fn reader() -> Sizes; }
  pub fn gap_between(sizes: &Sizes, prev: Option<&Md>, cur: &Md) -> f32;  // replaces gap_before
  pub struct ReaderDoc { pub blocks: Vec<Md>, pub plan: ReaderPlan }
  pub struct ReaderDraw { pub doc: Rc<ReaderDoc>, pub measure: f32 }   // later tasks add fields
  pub struct Draw { /* as today */ pub sizes: Sizes, pub reader: Option<ReaderDraw> }
  /// Row `row` of the Preview: the front matter or a block, in the centred column.
  pub fn reader_row(row: usize, draw: &Draw) -> AnyElement;          // replaces `row`
  // file_tab.rs (for scenes)
  pub fn reader_doc(&self) -> Option<Rc<ReaderDoc>>;
  ```
- Layout: a row is the Preview's full width with `TERMINAL_PADDING.0` sides (`TERMINAL_PADDING.1` over the first
  row and under the last), its content centred; prose (paragraphs, headings, lists, quotes, rules) at most
  `reader::MEASURE` wide; code blocks and tables at least the column and up to the area (plan-time call 7); a
  diagram at most the column (as today, centred). The front matter's row draws nothing yet (Task 6), but exists.
  H6 is `dim`. COLUMN_MAX goes.

- [x] **Step 1: Write the failing tests:**
  - `md_preview.rs` `chat_keeps_today_s_sizes`: `Sizes::CHAT` equals today's literal numbers;
    `gap_between(&Sizes::CHAT, …)` reproduces today's `gap_before` cases (the old test, moved).
  - `md_preview.rs` `the_reader_s_gaps`: with `Sizes::reader()`: para→para 16, para→heading 32, heading→para 12,
    item→item 6, first block 0.
  - Scene `file-tab-md-reader-column` (both themes, a 1,200 × 700 tab on `READER_FIXTURE` — a made-up "Setup guide"
    with front matter, two badge rows, `# Setup guide`, `## Install`, `## Configure`, `### Options`,
    `## Troubleshooting`, every callout kind, code in `rust`, `ts`, `sh` (one with a tab) and none, a 5-column table
    with a long cell, a Mermaid block, a long last section): Preview on; a paragraph's drawn width ≤ `MEASURE` and
    centred in the area (±1 pt); a wide table wider than that paragraph and inside the list's bounds; the body text
    at 16 pt (a shaped line's height ≈ 16 × 1.65); a 70-character English sentence shaped in the UI font at 16 pt is
    within 8 % of `MEASURE` (tune `MEASURE` if not, and record the value). A 500 pt-wide tab: nothing drawn past the
    list's right edge.
  - The existing agent chat Markdown scene (`ui_shots/agent.rs`, "Markdown in a reply") still passes untouched —
    the chat-unchanged proof.
- [x] **Step 2: Run** — FAIL. **Step 3: Implement** (move the Preview code first as its own step, gate green, then
  the reader's parse and layout).
- [x] **Step 4: Run** — PASS; the gate; the full `scripts/ui-shots.sh`.
- [x] **Step 5: Commit** `M11ai Task 3: the reader's column, type and spacing`.

### Task 4: Code blocks — colours, language, Copy

**Files:**
- Modify: `src/highlight.rs` (`highlight_lang`; `colour`'s line loop shared, File mode unchanged),
  `src/md_reader.rs` (`code_runs`), `src/md_preview.rs` (the header strip, coloured runs, Mermaid-as-code header),
  `src/file_tab_preview.rs` (the colour job and cache, Copy/Copied), `src/file_tab.rs` (`colour` also starts the
  code job when Preview shows), `src/tab_bar.rs` (`Glyph::Copy`, `Glyph::Check`), the reader scenes

**Interfaces:**
- Consumes: Task 2's `fence_lang`; Task 3's `ReaderDraw`, `reader_row`.
- Produces:
  ```rust
  // highlight.rs
  impl Highlighter { pub fn highlight_lang(&self, lang: &str, text: &str) -> Option<Vec<Vec<Span>>>; } // None: unknown
  // md_reader.rs
  /// A code block as drawn: its text with tabs widened (`file_tab::expand_tabs`), and its colour runs moved onto
  /// that text, each colour passed through `fit`.
  pub fn code_runs(text: &str, lines: &[Vec<Span>], fit: impl Fn(Rgb) -> Rgb) -> (String, Vec<(Range<usize>, Rgb)>);
  // md_preview.rs
  pub struct CodeLook { pub shown: SharedString, pub runs: Vec<(Range<usize>, Rgb)> }
  // ReaderDraw gains:
  pub code: Rc<HashMap<usize, Rc<CodeLook>>>,   // by block; missing: uncoloured
  pub copied: Option<Copied>,                   // #[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Copied { Code(usize), Anchor(usize) } (by row)
  pub on_copy: Rc<dyn Fn(usize, &mut Window, &mut App)>,   // a block's Copy pressed
  // Colours gains `code_header_bg` (code_bg mixed a shade toward the text) and `code_header_text` (dim at TEXT_MIN on it)
  // file_tab.rs (scenes)
  pub fn code_coloured(&self) -> usize;  pub fn copied(&self) -> Option<Copied>;  pub fn copy_bounds(&self, row: usize) -> Option<Bounds<Pixels>>;
  ```
- The job: on a new parse (and when the highlighter loads, and on a theme change) the tab collects
  `(block, fence_lang, text)` for every code block not drawn as a diagram, takes from its cache every
  `(lang, text)` already coloured in this theme, colours the rest on the background executor with the shared
  `Highlighter`, and accepts the answer only if the parsed version and theme are still the ones asked for (else
  drops it; a newer job runs). The cache keeps only the current version's entries. Copy writes `Md::Code::text`
  to the clipboard, sets `copied = Some(Code(row))` with a generation, and a one-shot 1.5 s timer clears it if the
  generation is unchanged. Element ids from the row (`("md-copy", row)`).

- [x] **Step 1: Write the failing tests:**
  - `highlight.rs` `the_fence_s_first_word_is_the_language`: `highlight_lang("rust", "fn main() {}")` has a
    keyword colour (palette 5) on `fn`; `"RUST"` and `"rs"` the same; `"ts"`, `"sh"` some colour other than the
    foreground; `an_unknown_language_is_plain`: `"foo"` → None. The existing `highlight.rs` tests unchanged.
  - `md_reader.rs` `code_runs_widen_tabs_and_keep_colours`: text `"\tlet x;\nfoo"` with spans covering `let` → shown
    `"    let x;\nfoo"`, the `let` run at `4..7`; `fit` applied to every run.
  - `md_preview.rs` (contrast test grows): `code_header_text` on `code_header_bg` at `TEXT_MIN` in every theme.
  - Scene `file-tab-md-reader-code` (both themes): after parking, `code_coloured()` = the fixture's coloured
    blocks; pixels of the `rust` block show the theme's keyword colour (fitted) inside its bounds; the header
    reads `rust` (a probe of the block's header text) and the block without a language has no language text; a
    click on Copy of the tab block → the clipboard holds the block's text with its `\t`; `copied()` =
    `Some(Code(row))` and the button's bounds hold the `Copied` label; shot `…-code-copied`. The Mermaid block,
    shown as code while waiting, has a `mermaid` header.
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate; the full ui-shots.
- [x] **Step 5: Commit** `M11ai Task 4: coloured code blocks with their language and Copy`.

### Task 5: Callouts

**Files:**
- Modify: `src/md_preview.rs` (callout drawing, `Colours::callouts`), `src/tab_bar.rs` (`Glyph::Info`, `Bulb`,
  `Report`, `Alert`, `Octagon`), the reader scenes

**Interfaces:**
- Consumes: Task 2's `BlockLook::Callout`, `Callout::title`.
- Produces: `Colours::callout(kind) -> CalloutColours { bar: Rgb, wash: Rgb, title: Rgb }` — the kind's palette
  colour (Note 4 blue, Tip 2 green, Important 5 magenta, Warning 3 yellow, Caution 1 red); `bar` at `MARK_MIN` on
  the wash, `wash` = the colour at `CALLOUT_WASH` over the background, `title` at `TEXT_MIN` on the wash; the body
  in `text` (held to `TEXT_MIN` on the wash).
- Drawn: a `CALLOUT_BAR` left bar, the wash, `CALLOUT_PAD` inside, `CALLOUT_RADIUS` corners, a title line (the
  kind's glyph and title, semibold, in `title`), then the body blocks with the reader's gaps; a body-less callout
  shows its title alone.

- [x] **Step 1: Write the failing tests:**
  - `md_preview.rs` `callout_colours_meet_their_floors_in_every_theme`: for each kind and theme, title and body text
    ≥ `TEXT_MIN` on the wash, bar ≥ `MARK_MIN` on the wash.
  - Scene `file-tab-md-reader-callouts` (both themes): each of the five callouts in the fixture: its title text
    present, a pixel of the bar in the kind's bar colour at the block's left edge, the wash behind the body; a
    `[!FOO]` quote still drawn as a quote (dim bar colour `line`).
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate; the full ui-shots.
- [x] **Step 5: Commit** `M11ai Task 5: GitHub callouts`.

### Task 6: Looks like GitHub — striped tables, front matter, badges

**Files:**
- Modify: `src/md_preview.rs` (table stripes and cell room, the front matter row, badge rows and pills, an inline
  badge's fill), the reader scenes

**Interfaces:**
- Consumes: Task 2's `Front`, `BlockLook::Badges`, `pill`, `is_badge_src`.
- Produces: `Colours` gains `stripe` (text at `STRIPE` over the background), `pill_bg`, `pill_value_bg` (a step
  stronger), `pill_text` (at `TEXT_MIN` on both fills); `Look::Badge` (an image in a sentence whose source is a
  badge's: its alt text on `pill_bg`).
- Drawn: tables — every 2nd, 4th… body row on `stripe`, header as today, cells `TABLE_CELL`; front matter — a
  two-column table, key `dim` at `FRONT_TEXT`, value `FRONT_TEXT`, table lines, no stripes, rounded, then a
  `+ N more` row; `Raw` as a code block labelled `front matter` (Task 4's header, no Copy); badge rows — pills
  `PILL_H` high, fully rounded, `PILL_TEXT`, `PILL_PAD` sides, `PILL_GAP` apart, wrapping; two-part pills split
  `label | value` with the value on `pill_value_bg`; empty alt `badge` dim; a linked pill follows its link through
  `on_link` (today's rules), its `src` never opened. No colour guessed from words.

- [x] **Step 1: Write the failing tests:**
  - `md_preview.rs` contrast test grows: table text on `stripe`, `pill_text` on `pill_bg` and on `pill_value_bg`, at
    `TEXT_MIN` in every theme.
  - Scene `file-tab-md-reader-github` (both themes): the table's 2nd body row's fill is `stripe`, the 1st's the page;
    the front matter row draws `title` / `Setup guide` and no rule or big heading above `# Setup guide`; the badge
    row draws one pill per badge, 20 pt high (bounds probe), the `license | MIT` pill with two fills; a click on the
    linked `build` pill reaches `on_link` with `https://example.com/ci` (a recorded link, no browser in shots); a
    file with 25 front-matter keys shows `+ 5 more`; a raw block shows a `front matter` header.
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate; the full ui-shots.
- [x] **Step 5: Commit** `M11ai Task 6: striped tables, front matter and badges`.

### Task 7: Heading anchors

**Files:**
- Modify: `src/md_preview.rs` (`LinkTarget::Here`, `File(PathBuf, Option<String>)`, `link_target`; the heading's
  hover `#`), `src/file_tab_preview.rs` (`follow_link`, `jump_to_row`, `show_anchor`, the copied link),
  `src/file_tab.rs` (`FileTabEvent::OpenFile(PathBuf, Option<String>)`; the workspace's handler), `src/agent_tab.rs`
  (compiler-led: its `link_target` match), the reader scenes

**Interfaces:**
- Consumes: Task 2's `anchor_row`, `jump_offset`, `heading_link`; Task 4's `Copied::Anchor`.
- Produces:
  ```rust
  pub enum LinkTarget { Web(String), File(PathBuf, Option<String>), Here(String), Nowhere }
  // file_tab_preview.rs
  pub(super) fn jump_to_row(&mut self, row: usize, cx: &mut Context<Self>);   // JUMP_ROOM under the top; shows the scrollbar
  pub fn show_anchor(&mut self, anchor: String, cx: &mut Context<Self>);    // Preview on, jump now or once parsed
  // ReaderDraw gains: pub on_anchor_copy: Rc<dyn Fn(usize, &mut Window, &mut App)>  (the heading's row)
  ```
- `follow_link`: `Here(a)` → `jump_to_row(anchor_row(a))` (unknown: nothing); `File(p, Some(a))` for a Markdown
  `p` → `OpenFile(p, Some(a))`; the workspace opens the tab (as today), turns its Preview on (saved, as a hand
  choice is) and calls `show_anchor(a)`; a non-Markdown file or no part → as today. The heading's `#` sits
  `ANCHOR_GAP` left of the text, shown on the heading's hover (a GPUI `group`); a click copies `heading_link(…)` and
  sets `copied = Some(Anchor(row))` (the `#` reads `Copied` for 1.5 s, the same one-shot).

- [x] **Step 1: Write the failing tests:**
  - `md_preview.rs` `links_go_to_the_web_a_file_beside_it_a_heading_or_nowhere` (replaces the old test): `#install`
    → `Here("install")`; `guide.md#install` → `File("/w/docs/guide.md", Some("install"))`; `../src/main.rs#L3` →
    `File(…, Some("L3"))`; `guide.md` → `File(…, None)`; the rest as before.
  - Scene `file-tab-md-reader-anchors` (one theme, state): a click on a `[see Configure](#configure)` link puts the
    Configure heading's row top + `jump_offset` at the list's top (`logical_scroll_top` = that row and offset);
    `#nope` changes nothing; hovering a heading shows its `#` (its bounds drawn), a click puts
    `docs/guide.md#install`-shaped text on the clipboard (the fixture's own path from its folder), `copied()` =
    `Anchor(row)`. A second tab: a link `other.md#middle` in a fixture opens `other.md` (via the workspace's
    handler in a workspace scene, or by calling `show_anchor` on a fresh tab) in Preview with `## Middle` at the
    top after parking.
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate; the full ui-shots.
- [x] **Step 5: Commit** `M11ai Task 7: heading anchors`.

### Task 8: "On this page" — the wide outline and the current entry

**Files:**
- Modify: `src/file_tab_preview.rs` (the outline column, `track_heading`, the width check, clicks),
  `src/file_tab.rs` (reader state fields), the reader scenes

**Interfaces:**
- Consumes: Task 2's `ReaderPlan::outline`, `current_entry`; Task 7's `jump_to_row`.
- Produces:
  ```rust
  // file_tab_preview.rs
  pub(super) fn track_heading(&mut self, cx: &mut Context<Self>);   // plan-time call 12; notifies only on a change
  pub fn outline_current(&self) -> Option<usize>;                  // scenes
  pub fn outline_shown(&self) -> Option<bool>;                     // Some(true) wide column, Some(false) button, None: no outline
  pub fn outline_entry_bounds(&self, entry: usize) -> Option<Bounds<Pixels>>;
  ```
- The column: `OUTLINE_W` wide on the Preview's right, `OUTLINE_GAP` from the reader area, the caption
  `On this page` (dim), entries `OUTLINE_TEXT` in `OUTLINE_ROW` rows, one line each ending in "…" with the full
  text as tooltip, H3 indented `OUTLINE_INDENT`; the current entry in the accent (at `TEXT_MIN` on the background)
  with an `OUTLINE_BAR` accent bar; the rest dim; hover `panel_hover`. A GPUI `list` of its own (only rows in sight
  drawn), its own scroll. The reader area shrinks by the column and its gap; the prose column centres in it.
  `reached(row)` = row above the list's top item, or its `bounds_for_item` top ≤ the viewport's top +
  `REACH` × its height; `in_view(row)` = drawn and its top < the viewport's bottom; `at_end` =
  `is_scrolled_to_end() == Some(true)`.

- [x] **Step 1: Write the failing tests:**
  - `md_preview.rs` contrast test grows: the outline's current entry (accent) and dim entries on the background.
  - Scene `file-tab-md-reader-outline` (both themes, a 1,200 × 700 tab, a long made-up doc with 12 sections):
    `outline_shown() == Some(true)`; at the top no entry current, after wheel steps until section 5's heading is in
    the top quarter → `outline_current() == Some(entry of section 5)` (and the accent bar's pixel at its row);
    scrolled to the end → the last heading in view is current; a click on entry 8 → section 8's row at the top
    (`jump_offset`), entry 8 current, the scrollbar shown; a wheel step that crosses no heading leaves
    `outline_current` unchanged. A doc with one H2 → `outline_shown() == None`; an 800 pt-wide tab → `Some(false)` (the threshold is 856 pt with today's padding).
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate; the full ui-shots.
- [x] **Step 5: Commit** `M11ai Task 8: the On this page outline`.

### Task 9: The narrow outline panel and the progress line

**Files:**
- Modify: `src/file_tab_preview.rs` (the button, the floating panel, Esc, the progress line in the canvas paint),
  `src/tab_bar.rs` (`Glyph::Outline`), the reader scenes

**Interfaces:**
- Consumes: Task 8's outline and `track_heading`; M11ae's `preview_track`.
- Produces: `pub fn outline_panel_open(&self) -> bool`, `pub fn outline_button_bounds(&self) -> Option<Bounds<Pixels>>`,
  `pub fn progress_share(&self) -> Option<f32>` (None: hidden).
- Narrow: a small `Outline` glyph button at the Preview's top-right (tooltip `On this page`); a click opens the
  outline (same entries, the same current entry) in `overlay_panel::shell` anchored under it (as
  `agent_box_menus.rs`'s `menu_frame` hangs its panel: `deferred(anchored())`, `on_mouse_down_out` closes it); an
  entry click jumps and closes it; Esc closes it (only while it is open; the tab's other Esc uses are unchanged).
  Preview off, ⌘F, a mode switch or the outline becoming wide closes it.
- Progress: `PROGRESS` pt along the Preview's top edge in the accent (at `MARK_MIN` on the background), its length
  the share `offset / (content − viewport)` from `preview_track`; hidden when the doc fits (no track).

- [x] **Step 1: Write the failing tests:**
  - Scene `file-tab-md-reader-narrow` (both themes, a 700 × 600 tab on the long doc): `outline_shown() ==
    Some(false)`, the button drawn; a click opens the panel (`outline_panel_open()`), shot `…-narrow-panel`; an
    entry click → that section at the top, the panel closed; open again, Esc → closed; open again, a click on the
    reader → closed.
  - Scene `file-tab-md-reader-progress` (both themes): at the top `progress_share() == Some(0.0)` (or ≈0) and no
    accent pixel along the top edge past 2 pt; scrolled half-way → share within 0.05 of the track's offset share,
    accent pixels along the top edge up to that share of the width and none past it; at the end ≈ 1.0; a doc that
    fits → `None`.
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate; the full ui-shots (--changed master only, by your gate change; the full suite ran once before the final review).
- [x] **Step 5: Commit** `M11ai Task 9: the narrow outline and the progress line`.

### Task 10: Reloads, big documents and the measurements

**Files:**
- Modify: `src/file_tab_preview.rs` (reload: place by row with the front-matter shift, colours kept, Copied
  cleared, the panel's entries refreshed, `track_heading` armed), `src/file_tab.rs` (`PREVIEW_MEASURE_ALL` if the
  measurement says so), the reader scenes, `src/md_reader.rs` (timing test from Task 2, kept)

**Interfaces:**
- Consumes: everything above.
- Produces: nothing new for later tasks.

- [x] **Step 1: Write the failing tests:**
  - Scene `file-tab-md-reader-reload` (one theme, state; Review Focus 4): scrolled to section 6 with Copied showing
    and the narrow panel open, the file rewritten with front matter added at the top → the same heading at the top
    (row + 1), `copied() == None`, `code_coloured()` unchanged at once (no wait) for unchanged blocks, the panel
    still open with the new entries, `outline_current` re-worked out; front matter removed → row − 1. A theme
    switch → colours recomputed for the new theme (cache keyed by theme).
  - Scene `file-tab-md-reader-big` (one theme, state): a 3,000-block file (over `PREVIEW_MEASURE_ALL`) in Preview:
    the outline's current entry follows wheel steps by block index; the progress share moves; nothing panics.
- [x] **Step 2: Run** — FAIL where behaviour is missing. **Step 3: Implement.**
- [x] **Step 4: Measure** (plan-time call 15): a temporary probe (not committed) laying out a 2,000-block mixed
  file in Preview, full relayout time, on this branch and on master (same file, debug); record both and the ratio;
  lower `PREVIEW_MEASURE_ALL` if over 1.15 ×. Record Task 2's parse + plan timing; move them to the background if
  over its limits. Numbers go into note 86 in Task 11.
- [x] **Step 5: Run** — PASS; the gate; the full ui-shots (--changed master only, by your gate change; the full suite ran once before the final review). **Commit** `M11ai Task 10: reloads and big documents`.

### Task 11: Hand-off — docs and the on-screen check

**Files:**
- Modify: `docs/04-Development/86 - Milestone 11ai - Markdown Reader Upgrade.md` ("What was built", "Calls made while
  building", "Known small gaps", "Measured headlessly", "Final numbers", "On-screen check" of about 12 items with
  `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`, debug only, made-up example docs; status "🟨 built on branch
  m11ai-md-reader, waiting for your on-screen check; not merged"), this plan (status table, ticks, "Tasks done"),
  `docs/04-Development/40 - Roadmap.md`, `docs/06-Progress-Log/60 - Progress Log.md`; remove any
  `#![allow(dead_code)]` left in `md_reader.rs`
- On-screen items include idle CPU with a long doc shown (`top -l 6 -s 5 -pid <pid>`, branch vs master) and the
  outline's feel while scrolling; the user runs them, not the agent.

- [x] **Step 1:** Write the docs; the gate; the full ui-shots (--changed master only, by your gate change; the full suite ran once before the final review); the numbers into note 86.
- [x] **Step 2:** Commit `M11ai Task 11: hand-off docs` (explicit paths).

## Self-review (done at plan time)

- Spec coverage: column/type/spacing (3), code colours/language/Copy/Mermaid header (4), callouts (5), striped
  tables/front matter/badges (6), anchors incl. cross-doc and copy link (7), outline wide + tracking (8), narrow
  panel + progress (9), live reload/big docs/perf/theme cache (10), contrast test growth (4–8), light and dark
  shots (each scene `Themes::Both` where visual), chat unchanged (3), parser additive (1).
- Names checked across tasks: `ReaderPlan`, `RowKind`, `ReaderDoc`, `ReaderDraw`, `Copied`, `jump_to_row`,
  `show_anchor`, `track_heading`, `outline_current`, `code_coloured`, `copy_bounds`.
