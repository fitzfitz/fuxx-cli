---
tags: [development, plan, milestone]
milestone: M11ag
status: built
updated: 2026-11-19
---

# M11ag Plan — Ask the agent from the code

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[87 - Milestone 11ag - Ask the Agent]] |
| **Status** | 🟨 built on branch m11ag-ask-agent, waiting for your on-screen check; not merged (all tasks built; the whole-branch review's fix wave is in and re-reviewed clean; 2,154 tests passed, 1 ignored; full ui-shots 247 passed, 0 failed, 158.3 s) |
| **Tasks done** | 13 / 13 (plus Task 0); Tasks 0–12 reviewed |
| **Baselines** | On `m11ag-ask-agent` at `687264d`, 2026-11-19: `cargo test --bin fuxx` 2,076 passed, 1 ignored, 0 failed; clippy default 14 (bin) / 27 (test); clippy `ui-shots` 16 (bin) / 29 (test); `cargo fmt --check` clean. Full `scripts/ui-shots.sh` not run in Task 0 (ruling R1; the user's decision): the ui-shots baseline is M11ai's last full run, 226 passed, 0 failed, 151.4 s |
| **▶ Pick up here** | Your on-screen check in [[87 - Milestone 11ag - Ask the Agent]] (15 items); then the merge on your word |
| **Decisions used** | Yours: "lets go" to the 11 points and **B** (no agent tab: open one), "go with A" (note rows in the code), "go ahead" (lines from the edit itself, 7b/7c/10), the full ui-shots run only before the final review and after its fix wave. The design's "Calls made" (AI's). Plan-time calls below (AI's). |
| **Needs your eyes** | 👀 the on-screen check in note 87, after Task 13 |

**Goal:** Select lines in a file tab and, with one key or click, have them waiting as a code chip in an agent tab's
box (or copy them with their path); and read the agent's own reason for an edit as a note row under the lines it
changed.

**Architecture:** Part 1: a pure `src/code_ask.rs` turns a selection (File or Diff mode) into a `CodeRef` (path,
whole lines, fence, body); the file tab's selection bar (`src/file_tab_ask.rs`, a `#[path]` child module as
`file_tab_preview.rs` is) emits `FileTabEvent::Ask(CodeRef)` and copies; the Workspace picks the agent tab (a new
per-workspace "last agent tab" record), attaches `Attachment::Code` and focuses its box; the text is made at send
time by `wire_text`. Part 2: providers hand generic `EditText`s on `Event::ToolStarted`; the agent tab catches the
turn's last text as the reason (`edit_events`); the Workspace reads the edited file off the UI thread and files
notes in an app-wide `NoteStore` (`src/edit_notes.rs`, beside `edit_ledger`); each file tab places its file's notes
off the UI thread after each read, and `RowMap` gains `Row::Note` rows that the body paints
(`src/file_tab_notes.rs`) and every other consumer steps around.

**Tech Stack:** Rust 2024, GPUI (pinned rev `1d2a4b3f`, unchanged). Note wrapping uses
`window.text_system().shape_text(text, size, &runs, Some(width), None) -> SmallVec<[WrappedLine; 1]>` and
`WrappedLine::wrap_boundaries()` / `WrappedLine::paint(..)`, read from the pinned source
(`crates/gpui/src/text_system.rs:509`, `text_system/line.rs:257`); check Context7 or that source before any GPUI
API this repo doesn't use yet. **No new crates.**

**Spec:** [[87 - Milestone 11ag - Ask the Agent]] (binding). Read [[85 - Milestone 11ah - Code View Upgrade]] (the
row map) and [[82 - Milestone 11af - Live Changes]] (the ledger and the line record) for the code as it is.

## Plan-time calls (AI's, made while reading the code)

1. **agy carries no edit text (checked at plan time).** All three agy recordings with an edit
   (`src/providers/fixtures/agy-tools.jsonl`, `agy-restart-flags.jsonl`, `agy-restart-flags-effort.jsonl`) give
   `edit_file` / `write_file` only `{"Path": …}`. So agy edits get notes the light way (7b). Task 0 re-checks; if a
   recording does carry the new text, Task 6 Step 4b applies instead. **Task 0 re-check (2026-11-19): confirmed.**
   Eight `edit_file` / `write_file` calls across the three recordings (`agy-tools.jsonl`: 2 edit, 2 write;
   `agy-restart-flags.jsonl`: 2 write; `agy-restart-flags-effort.jsonl`: 2 write); each one's `tool_info.parameters`
   has the single key `Path`. No key besides `Path` in any of them, and no new-text-style key (content, replacement,
   new string, text, diff, patch) anywhere in the `agy-*.jsonl` fixtures. Task 6 Step 4b does not apply.
2. **No keyboard selection exists in the file tab** (no ⇧-arrows; ⌘A is the Edit menu's). The bar shows when a
   mouse selection ends (`mouse_up`) and the selection is not empty.
3. **Esc does not clear the selection today** (`close_find` only closes the find bar or the outline panel, else
   propagates). New: with neither open, Esc clears a selection (and so hides the bar); with no selection it still
   propagates.
4. **The chip keeps the absolute path; the agent's text is made at send time** (`wire_text` gains the tab's `cwd`),
   so a folder changed between Ask and send is honoured. The code's lines are captured at Ask.
5. **Text format** (exact): header line, then a fence. Header `"{shown}:{first}-{last}"`, one line
   `"{shown}:{first}"`, removed-only `"{shown} (removed, was {first}-{last})"` / `"(removed, was {first})"`, ASCII
   hyphen. Chip label: the file name, en dash: `"workspace.rs:43–45"`, `"workspace.rs:43"`,
   `"workspace.rs (removed, was 12–14)"`. Diff bodies use ASCII `+`, `-`, space (the screen's U+2212 is a display
   glyph; a `diff` fence needs `-`). Lines lose `\r`; tabs are kept as tabs.
6. **The fence language** is the syntax name the colouring found (`Doc::syntax`), lowercased, when it is one word of
   letters, digits, `+` or `#` (`Rust` → `rust`, `C++` → `c++`); else the extension lowercased (`Bourne Again Shell
   (bash)` for `x.sh` → `sh`; no syntax yet, or a text too large to colour: the extension too); none for
   `Plain Text` and for a file with no extension and no syntax.
7. **File mode takes the hidden lines of a fold inside the selection** (as ⌘C does today); Diff mode leaves out the
   lines of a folded `⋯` stretch or a closed card (spec 6), and Gap rows.
8. **A new agent tab still waiting for its view is a target**: `pick_agent_tab` counts a tab in the model with no
   slot yet as attachable, so two quick Asks fill one new tab, not two.
9. **The bar's label is pushed, not polled**: `Workspace::tell_ask_targets` calls `FileTab::set_ask(AskTo, cx)` from
   `note_active` (every focus change, tab open, close and move goes through `activate`), `make_file_tab` and
   `make_agent_tab`; `set_ask` notifies only on a change.
10. **The last agent tab** is recorded in `note_active` (and in `ensure_agent_tabs` when a new view takes the
    keyboard, and by Ask itself): per workspace (`last_agent`), and its provider app-wide
    (`last_agent_provider`). Not saved.
11. **Where an edit's new text is**: its first occurrence in the file read at the finish (an `Edit`'s new text is not
    always unique; `replace_all` is "first match" by the spec anyway). The anchor is the **whole lines** the new text
    spans then; the hint is its first line. A text not found: no note.
12. **One note per reason per file** shows under the **topmost** placed block of that reason.
13. **The light way** (pure deletions, and agy): at a successful finish with no findable text, the Workspace leaves a
    `Light` reason in the store for the path and agent; it lasts `edit_ledger::KEEP` (600 s). A file tab whose live
    record stamps its latest change **exactly** to that agent (in `live_saw`, or later at the settle / revise check)
    takes the newest one (all pending for that path and agent go) and files a note anchored at the change's first
    block, or, for a deletion only, at the line above the first deleted edge (line 1 for an edge at the top).
14. **Notes are placed for a text that isn't `plain`** (`compare_with`'s cut); the placement gives up after
    `live_lines::LIMIT` (50 ms): no notes for that read.
15. **A note's rows**: folded `min(2, n)`, open `n`, where `n` is the wrapped line count of
    `"{agent} · {clock} — {reason}"` in the UI font (`text::UI`) at the code's visible text width; "more" / "less" only
    when `n > 2`. `RowMap::MapKey` gains the notes generation and that width, so rows are rebuilt only then, in the
    prepaint as today.
16. **Note under-lines in Diff mode**: the diff row whose `DiffRow.new == Some(under + 1)`; none (a removed-only
    neighbourhood): the note isn't shown in Diff mode.
17. **The pill counts the notes placed** in the tab's text, shown or hidden by the eye (the pill stays, so the eye
    can turn them back on); it shows in File and Diff modes, not in Preview. "Next note" is the first note whose
    first row starts below the view's bottom edge, else the first note; it scrolls the note's under-line to the
    top as Find's reveal does (`reveal_top`).
18. **Copy for the new words** (exact): `Ask {agent} ⌘↵` (`⌘↵` as `\u{2318}\u{21b5}`), `Too long to ask`,
    `Copy with path`, `more`, `less`, `1 note from claude`, `2 notes from claude`, `2 notes` (both agents), the eye's
    tooltips `Hide notes` / `Show notes`, the chip's glyph `</>`, the note prefix `claude · 10:42 — ` (`·` U+00B7,
    `—` U+2014, spaces as shown).
19. **Clock**: made once when the note is filed, `activity::clock_hm(unix, cli::local_utc_offset(ms))`, kept as a
    string on the note.

## Global Constraints

- Branch `m11ag-ask-agent`, main checkout. Commit per task (stage files by path); no merge, no push in the build.
- **Gate per task:** `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features
  ui-shots && cargo fmt --check` (no new warnings over Task 0's baselines: 14 / 27 default, 16 / 29 `ui-shots`), then
  `scripts/ui-shots.sh --changed master`. **The full `scripts/ui-shots.sh` runs only before the final review and
  after its fix wave** (your decision). A failing scene is re-run alone once before it counts. **Never
  `--update-refs`.**
- **M11p:** nothing visible changes inside any `render`; no always-on timer (no `timer(` loop,
  `request_animation_frame` or `spawn` loop added). Note rows are built when the notes or the line record change,
  in the prepaint's map rebuild, never per frame; times are clock strings ("10:42"), so nothing ticks.
- **Provider-specific code only in `src/providers/<agent>.rs`**; generic code sees `EditText` and `Event`s.
- **Read-only viewer** ([[12 - Scope]]): no editing, no language servers, no services; Ask never sends.
- **Nothing is sent without your send:** no Ask path may call `send`, `say` or write to the agent's process.
- **Public repo:** example paths are fuxx-cli's own (`src/workspace.rs`, `src/file_tab.rs`), never other projects'
  names; no `/Users/` paths, personal names, emails or time-zone names in code, scenes or docs. Grep each commit's
  added lines for `/Users/` before it lands. No Co-Authored-By or AI attribution lines.
- **Shell safety:** never run the app by the bare name `fuxx` (the user's real app is on PATH); single-quote grep
  patterns; never put backticks or `$(...)` inside double-quoted shell strings. The app, if ever run, only as
  `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug, never `--release`).
- **Looks:** sizes and colours from `ui_tokens` and the theme (no hex); the bar's buttons match M11al's text buttons;
  text at `TEXT_MIN` on what it is drawn on, bars and marks at `MARK_MIN`. Every visible part has a ui-shots scene,
  `Themes::Both` where it reads pixels.
- New scenes go in two new area files, `src/ui_shots/ask.rs` (Part 1) and `src/ui_shots/notes.rs` (Part 2), each
  with `FILE` / `SCENES`, a `mod` in `src/ui_shots/mod.rs`, an `AREAS` row and names in `registry::ORDER` (after the
  `file-follow-*` scenes); chip scenes go in `src/ui_shots/agent_box.rs`.
- Agents can't see the window: GUI behaviour is proven by scene probes (bounds, pixels, state) and said so in
  reports; the look is the user's on-screen check.

## Review Focus

1. **A selection that spans hidden lines** (a File-mode fold; a Diff folded `⋯` stretch or closed card; a selection
   ending on a line's first column): File takes every line `first..=last` including the fold's (as ⌘C); Diff leaves
   out held lines and Gap rows; a selection ending at column 0 of a line still takes that line (whole lines, spec 4).
   Pinned in Task 1 (`a_fold_inside_the_selection_is_taken`, `a_selection_ending_at_column_zero_takes_that_line`)
   and Task 5 (`held_and_gap_rows_are_left_out`).
2. **Two Asks in a row with no agent tab**, the second before the new tab's view exists, then the new tab closed
   before its view is made: one new tab with both chips (deduped by place), and closing it drops the waiting chips
   with no panic. Pinned in Task 3 (`a_waiting_new_tab_is_a_target`, scene `ask-new-tab`'s second Ask and close).
3. **A file path outside the agent's folder, outside any repo and outside home, with no extension, CRLF endings and a
   tab** (e.g. a scratch `Makefile` under the temp folder): the agent text's path is absolute, no fence language for
   an unknown name (`Makefile` is known: `makefile`), no `\r` in the body, the tab kept. Pinned in Task 1
   (`paths_relative_inside_the_folder_else_absolute`, `crlf_lines_lose_their_cr_and_keep_tabs`).
4. **The anchor text appears twice in the file, and a later edit moves it** (lines inserted above): the note goes to
   the copy nearest its hint, and after the insertion still to the moved block (nearest the old hint wins over a
   farther identical block). Pinned in Task 9 (`the_anchor_nearest_the_hint_wins`, scene `notes-follow-edit`).
5. **A reason with multi-byte text at the cap and Markdown in it** (`**bold**`, a code fence, 3,000 chars of `é`):
   cut at 2,000 characters on a char boundary, whitespace runs one space, shown as the raw characters (no Markdown
   rendering). Pinned in Task 7 (`reason_text_collapses_whitespace_and_caps_on_a_char_boundary`) and Task 11 (scene
   `notes-row` draws the raw `**`).

---

### Task 0: Baselines and the agy check

- [x] **Step 1:** On `m11ag-ask-agent` at the plan commit: `cargo test --bin fuxx` (count passed / ignored), both
  clippies (warning counts), `cargo fmt --check`, and the full `scripts/ui-shots.sh` (passed / failed / seconds).
  Write the numbers into this plan's "Baselines" row.
- [x] **Step 2:** Re-check agy: `grep -l 'edit_file' src/providers/fixtures/agy-*.jsonl`, and for each line with
  `edit_file` or `write_file` list the keys of `tool_info.parameters`. Record in "Plan-time calls" 1 whether any
  key besides `Path` carries the new text (expected: none). No new recording is made (it would mean running agy).
- [x] **Step 3:** Commit `M11ag Task 0: baselines` (this plan only).

### Task 1: Code references (pure)

**Files:**
- Create: `src/code_ask.rs` (module doc; `mod code_ask;` in `src/main.rs`)
- Modify: `src/md_reader.rs` (`shown_path` split out of `heading_link`)

**Interfaces:**
- Produces (`code_ask`):
  - `pub const ASK_MAX_LINES: usize = 500; pub const ASK_MAX_BYTES: usize = 64 * 1024;`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub enum CodeLines { Lines { first: usize, last: usize }, Removed {
    first: usize, last: usize } }` (1-based, inclusive; `Removed` names old lines)
  - `#[derive(Clone, Debug, PartialEq)] pub struct CodeRef { pub path: PathBuf, pub real: Option<PathBuf>, pub
    lines: CodeLines, pub fence: Option<String>, pub body: String }`
  - `impl CodeRef { pub fn label(&self) -> String; pub fn header(&self, shown: &str) -> String; pub fn text(&self,
    shown: &str) -> String; pub fn too_long(&self) -> bool; pub fn same_place(&self, other: &CodeRef) -> bool }`
    (`text` = header, `\n`, ```` ``` ```` + fence word, `\n`, body, `\n`, ```` ``` ````; `too_long`: body lines >
    `ASK_MAX_LINES` or body bytes > `ASK_MAX_BYTES`; `same_place`: same `path` and `lines`)
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum AskTo { Agent(&'static str), CopyOnly }` (default
    `Agent("claude")`)
  - `pub fn whole_lines(a: Pos, b: Pos, last: usize) -> (usize, usize)` (0-based inclusive, either order, clamped)
  - `pub fn file_ref(path: &Path, real: Option<&Path>, doc: &Doc, a: Pos, b: Pos) -> Option<CodeRef>` (None when
    `a == b`)
  - `pub fn fence_lang(syntax: Option<&str>, path: &Path) -> Option<String>` (plan-time call 6)
  - `pub fn agent_path(path: &Path, real: Option<&Path>, cwd: &Path) -> String` (relative when `path`, else `real`,
    is strictly inside `cwd`; else `path` whole)
  - `pub fn copy_text(code: &CodeRef, rel: Option<&Path>, home: Option<&Path>) -> String` (=
    `code.text(&md_reader::shown_path(rel, &code.path, home))`)
- Produces (`md_reader`): `pub fn shown_path(rel: Option<&Path>, path: &Path, home: Option<&Path>) -> String`;
  `heading_link` becomes `format!("{}#{slug}", shown_path(..))` (its test unchanged).
- Consumes: `file_tab::{Pos, Doc}` (`Doc::new(text, 0)` builds test docs).

- [x] **Step 1: Write the failing tests** (in `code_ask.rs`), with a `src/workspace.rs`-named made-up doc:
  - `labels_and_headers`: lines 43–45 → label `"workspace.rs:43–45"`, `header("src/workspace.rs")` ==
    `"src/workspace.rs:43-45"`; one line → `"workspace.rs:43"` / `":43"`; `Removed{12,14}` → `"workspace.rs
    (removed, was 12–14)"` / `"src/workspace.rs (removed, was 12-14)"`.
  - `the_text_is_header_then_fence`: `text("src/workspace.rs")` == the spec's example exactly (three lines of Rust
    in a ```` ```rust ```` fence).
  - `whole_lines_take_every_line_touched`: `whole_lines(Pos{line: 42, col: 7}, Pos{line: 44, col: 2}, 99) ==
    (42, 44)`; reversed order equal; `a_selection_ending_at_column_zero_takes_that_line`; `a_fold_inside_the_selection_is_taken`
    (`file_ref` over lines 10..=20 returns 11 lines; folds are not an input).
  - `crlf_lines_lose_their_cr_and_keep_tabs`: body has no `\r`, `\t` kept.
  - `the_cap_is_500_lines_or_64_kb`: 500 lines → not too long; 501 → too long; 300 lines of 300 bytes → too long.
  - `fence_lang_from_the_syntax`: `("Rust", a.rs)` → `rust`; `("C++", a.cpp)` → `c++`; `("Bourne Again Shell
    (bash)", x.sh)` → `sh`; `("Plain Text", x.zzz)` → None; `(None, Makefile)` → None; `(None, a.rs)` → `rs`.
  - `paths_relative_inside_the_folder_else_absolute`: `/p/fuxx-cli/src/workspace.rs` in `/p/fuxx-cli` →
    `src/workspace.rs`; outside → whole; `real` used when `path` isn't inside but `real` is; the folder itself is not
    "inside".
  - `copy_text_uses_the_repo_relative_path`: rel `src/workspace.rs` → header `src/workspace.rs:43-45`; no rel, under
    home → `~/…`; neither → absolute.
  - `same_place_is_path_and_lines`.
- [x] **Step 2: Run** `cargo test --bin fuxx code_ask` — FAIL (module missing).
- [x] **Step 3: Implement** `code_ask.rs` and `md_reader::shown_path` (plan-time calls 5, 6, 7).
- [x] **Step 4: Run** — PASS; the gate.
- [x] **Step 5: Commit** `M11ag Task 1: code references`.

### Task 2: The code chip in the agent box

**Files:**
- Modify: `src/agent_tab.rs` (`Attachment::Code`, `label`, `glyph`, `attach` dedupe, `wire_text` with `cwd`, its
  call in `say`, tests), `src/agent_box.rs` (`attachment_chips`: the code chip's tooltip), `src/ui_shots/agent_box.rs`

**Interfaces:**
- Consumes: Task 1's `CodeRef`, `agent_path`.
- Produces: `Attachment::Code(CodeRef)`; `label()` → `code.label()`; `glyph()` → `"</>"`;
  `pub fn wire_text(text: &str, attachments: &[Attachment], images_inline: bool, cwd: &Path) -> String` (a code
  chip says `code.text(&agent_path(&code.path, code.real.as_deref(), cwd))`); `AgentTab::attach` ignores a
  `Code` whose `same_place` a waiting one's.

- [x] **Step 1: Write the failing tests** (`agent_tab.rs`): `wire_text_gives_code_with_its_path_from_the_agent_folder`
  (`"why?"` + a code chip, cwd `/p/fuxx-cli` → `"why?\n\nsrc/workspace.rs:43-45\n```rust\n…\n```"`);
  `the_same_code_twice_is_one_chip` (pure: a helper `fn attachable(list: &[Attachment], new: &Attachment) -> bool`
  used by `attach`); the old `wire_text_*` tests pass `cwd` and keep their expectations.
- [x] **Step 2: Write the failing scene** `agent-box-code-chip` (after `agent-box-file-chip` in `ORDER`, `Both`,
  Visual): a scene tab gets two Asks' worth of chips (one repeated): one chip, label `"workspace.rs:43–45"` with
  `</>`; hovering it shows a tooltip with the full path; × removes it; with a chip, send `"why?"` → the scene tab's
  `written` holds the wire text above; before the send nothing is written.
- [x] **Step 3: Run** `cargo test --bin fuxx agent_tab` and `scripts/ui-shots.sh agent-box-code-chip` — FAIL.
- [x] **Step 4: Implement.** **Step 5: Run** — PASS; the gate.
- [x] **Step 6: Commit** `M11ag Task 2: the code chip`.

### Task 3: Which agent tab, and Ask into it

**Files:**
- Modify: `src/workspace.rs` (fields `last_agent: HashMap<WorkspaceId, TabId>`, `last_agent_provider:
  Option<&'static str>`, `pending_chips: HashMap<TabId, Vec<Attachment>>`; `note_active` records and calls
  `tell_ask_targets`), `src/workspace_agent.rs` (`pick_agent_tab`, `ask_target`, `ask_code`, `tell_ask_targets`,
  `new_agent_tab` / `add_agent_tab` return the tab, `make_agent_tab` drains `pending_chips`, `ensure_agent_tabs`
  records), `src/file_tab.rs` (field `ask: AskTo`, `pub(crate) fn set_ask(&mut self, to: AskTo, cx)`; the
  Workspace's `make_file_tab` calls `tell_ask_targets`), `src/ui_shots/ask.rs` (new area), `src/ui_shots/mod.rs`,
  `src/ui_shots/registry.rs`

**Interfaces:**
- Consumes: Task 1's `CodeRef`, `AskTo`; Task 2's `Attachment::Code`, `attach`.
- Produces:
  - `pub(crate) fn pick_agent_tab(tabs: &[(TabId, bool)], last: Option<TabId>) -> Option<TabId>` (tabs in
    `Model::agent_tabs` order, bool = attachable: `slot.attachable`, or no slot yet; `last` if listed and
    attachable, else the first attachable)
  - `pub(crate) fn ask_agent(picked: Option<&'static str>, last_anywhere: Option<&'static str>) -> &'static str`
    (picked tab's provider name, else the last anywhere, else `"claude"`)
  - `Workspace::ask_target(&self, ws: WorkspaceId, cx: &App) -> AskTo` (`CopyOnly` when `workspace_is_ssh(ws)`)
  - `Workspace::ask_code(&mut self, file_tab: TabId, code: CodeRef, window: &mut Window, cx: &mut Context<Self>)`:
    SSH → nothing; a picked tab with a view → `attach`, `select_tab`, `sync`, `activate` (its composer takes the
    keyboard, the file tab goes behind it in the same pane); a picked tab still waiting → push to
    `pending_chips[tab]`, select it; none → `new_agent_tab(file tab's pane, ask_agent(..))`, chip to
    `pending_chips[new]`. Records `last_agent` / `last_agent_provider`. Never sends.
  - `new_agent_tab(..) -> Option<TabId>`, `add_agent_tab(..) -> Option<TabId>` (callers ignore it as before).
  - `pending_chips` entries of tabs no longer in the model are dropped in `sync`.

- [x] **Step 1: Write the failing tests** (`workspace_agent.rs`): `pick_takes_the_last_used_then_reading_order`;
  `pick_skips_tabs_that_failed`; `a_waiting_new_tab_is_a_target`; `ask_agent_falls_back_to_last_anywhere_then_claude`.
- [x] **Step 2: Write the failing scenes** (`src/ui_shots/ask.rs`, State; drive `ask_code` directly, with a made-up
  `src/workspace.rs` doc under a scratch `fuxx-cli` folder):
  - `ask-existing-tab`: a workspace with a file tab and two agent tabs (claude, agy), the agy tab focused last →
    after `ask_code` the agy tab holds the chip, is its pane's active tab and focused (its composer has the
    keyboard), its scene `written` is empty; the claude tab got nothing. Same pane case: file tab and agent tab in
    one pane → the agent tab is active after Ask. The file tab's `ask()` probe reads `Agent("agy")`.
  - `ask-new-tab`: no agent tab, `last_agent_provider = Some("agy")` (set by focusing an agy tab in another
    workspace) → one new agy tab in the file tab's pane, its chip present on the frame after its view is made; a
    second `ask_code` before that frame → still one new tab, two chips (different lines) or one (same lines); a
    third run closes the new tab before its view exists → no chip kept, no panic. With no record anywhere → a
    claude tab.
- [x] **Step 3: Run** — FAIL. **Step 4: Implement** (plan-time calls 8, 9, 10).
- [x] **Step 5: Run** — PASS; the gate; `scripts/ui-shots.sh --changed master`.
- [x] **Step 6: Commit** `M11ag Task 3: Ask finds its agent tab`.

### Task 4: The selection bar, ⌘↵ and Copy with path (File mode)

**Files:**
- Create: `src/file_tab_ask.rs` (`#[path = "file_tab_ask.rs"] mod ask;` in `file_tab.rs`, `use super::*`)
- Modify: `src/file_tab.rs` (action `AskAgent` in `actions!`; `FileTabEvent::Ask(CodeRef)`; the body draws the bar;
  `mouse_up` / `close_find` / scroll as below; the Workspace's subscriber handles `Ask`), `src/main.rs`
  (`KeyBinding::new("cmd-enter", file_tab::AskAgent, Some("FileTab"))`, keymap test), `src/ui_shots/ask.rs`

**Interfaces:**
- Consumes: Task 1's `file_ref`, `copy_text`, `CodeRef::too_long`; Task 3's `ask`, `ask_code`.
- Produces:
  - `const BAR_GAP: f32 = 6.0` (the bar's height: M11al's text button height)
  - `pub fn sel_bar_y(first_top: f32, last_bottom: f32, view_h: f32, bar_h: f32, gap: f32) -> Option<f32>` (y in
    the text area: below the last row when `last_bottom` is in `(0, view_h]` and the bar fits below; else above the
    first row when it fits above; else inside at `view_h - gap - bar_h`; None when the last row is off screen)
  - `FileTab::code_ref(&self) -> Option<CodeRef>` (File mode here; Diff in Task 5), `fn bar_shown(&self) -> bool`
    (a non-empty selection, not `selecting`, Plain or Diff, Preview off, a text), `fn sel_bar_at(&self) ->
    Option<Point<Pixels>>` (window point of the bar's top-right corner: from `geom`, the last `rows` map and the
    mode's scroll; the x is the code's visible right edge), `fn ask(&mut self, cx)` (emits `Ask` unless `CopyOnly`,
    no selection or too long), `fn copy_with_path(&mut self, cx)` (clipboard = `copy_text(code, git_place.rel,
    home)`).
  - ui-shots probes: `sel_bar_bounds() -> Option<Bounds<Pixels>>`, `ask_button() -> Option<(String, bool)>`
    (words, enabled), `copy_button_bounds()`.

- [x] **Step 1: Write the failing tests:** `sel_bar_y_below_above_inside_and_hidden` (view 400, bar 28, gap 6: last
  bottom 100 → 106; last bottom 390, first top 200 → 166; first top 10, last bottom 395 → 366; last bottom 420 →
  None; last bottom ≤ 0 → None); in `main.rs` `cmd_enter_asks_in_the_file_tab_only` (bound in
  `["Workspace","FileTab"]`, not in a terminal, page or the find box `["Workspace","FileTab","NameInput"]`).
- [x] **Step 2: Write the failing scenes** (`ask.rs`, `Both`, Visual unless said):
  - `ask-bar`: a 60-line file in a workspace with one claude agent tab, drag 43:7 → 45:2 and release → the bar's
    top-right is at the code's right edge, its
    top 6 pt (the gap) under row 45's bottom; buttons `"Ask claude ⌘↵"` and `"Copy with path"`; no code row moved
    (row 46's top unchanged); Copy with path → the clipboard is the spec's text with `src/workspace.rs:43-45`;
    ⌘↵ (dispatched as a key) → the agent tab holds the chip, nothing written; with no selection ⌘↵ does nothing.
  - `ask-bar-above`: the selection's last row at the view's bottom row → the bar's bottom is 6 pt above the first
    row's top.
  - `ask-bar-hides` (State): while dragging (`selecting`) → no bar; scrolled until the last row is off screen → no
    bar, back → bar; Esc → selection None and no bar; a second Esc propagates.
  - `ask-bar-agy`: last-used agent tab agy → `"Ask agy ⌘↵"`; that tab closed → the label follows the next pick.
  - `ask-bar-ssh` (State): the file tab in an SSH workspace (a scene SSH session) → only `"Copy with path"`; ⌘↵ does
    nothing (no tab opened).
  - `ask-bar-too-long`: a 501-line selection → `ask_button() == Some(("Too long to ask", false))`, click does
    nothing, ⌘↵ does nothing; Copy with path copies all 501 lines.
- [x] **Step 3: Run** — FAIL. **Step 4: Implement** (plan-time calls 2, 3, 18). Draw the bar as M11af's `mark_tip`
  is drawn (`deferred(anchored().position(at).anchor(Corner::TopRight))` in `body`); the position is read in
  render from state only (no writes). When the prepaint rebuilds the map while a bar is shown, defer one
  `cx.notify()` (as `fits_side` does) so the bar follows a reflow.
- [x] **Step 5: Run** — PASS; the gate; `--changed master`.
- [x] **Step 6: Commit** `M11ag Task 4: the selection bar`.

### Task 5: Ask and Copy from Diff mode

**Files:**
- Modify: `src/code_ask.rs` (`diff_ref`, tests), `src/file_tab_ask.rs` (`code_ref` in Diff mode, unified and side by
  side), `src/ui_shots/ask.rs`

**Interfaces:**
- Consumes: `diff_view::{DiffRow, RowKind}`, `row_map::{Half, RowMap::held}`.
- Produces: `pub fn diff_ref(path: &Path, real: Option<&Path>, doc: &Doc, rows: &[DiffRow], held:
  &[Range<usize>], half: Option<Half>, a: Pos, b: Pos) -> Option<CodeRef>` — whole diff rows `first..=last`, minus
  `held` lines and Gap rows; side by side (`half` Some) only that half's rows (Old: Same + Removed; New: Same +
  Added); body lines `"+"`/`"-"`/`" "` + text; fence `"diff"`; lines = `Lines{min new, max new}` over kept rows with
  a `new`, else `Removed{min old, max old}`; None when no row is kept.

- [x] **Step 1: Write the failing tests:** `diff_ref_names_the_current_lines` (rows Same 41/43, Removed 42/–, Added
  –/44, Same 43/45 → `Lines{43,45}`, body `" a\n-b\n+c\n d"`); `removed_only_is_named_after_the_old_lines`
  (`Removed{12,14}`, label `"workspace.rs (removed, was 12–14)"`); `held_and_gap_rows_are_left_out`;
  `side_by_side_takes_only_its_half`.
- [x] **Step 2: Write the failing scene** `ask-diff` (`Both`): a made-up diff (`set_diff_for_shots`) with two cards
  and a folded stretch; a selection from card 1's last rows across the `⋯` row and card 2's title into card 2 →
  the bar shows; Copy with path gives a ```` ```diff ```` fence without the stretch's lines or a title; the label
  names the new lines spanned; a removed-only selection → `(removed, was …)`; side by side, a selection on the left
  half → only Same + Removed rows; ⌘↵ attaches it.
- [x] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; `--changed master`.
- [x] **Step 6: Commit** `M11ag Task 5: Ask from Diff mode`.

### Task 6: Providers hand over the edit's new text

**Files:**
- Modify: `src/providers/mod.rs` (`EditText`, `Event::ToolStarted.texts`), `src/providers/claude.rs`
  (`tool_texts`, `assistant`), `src/providers/agy.rs` (`texts: Vec::new()` with a comment citing Task 0), every
  other `Event::ToolStarted { .. }` construction the compiler finds (tests, `src/ui_shots/*`, history readers)

**Interfaces:**
- Produces: `#[derive(Clone, Debug, PartialEq)] pub enum EditText { Inserted(String), WholeFile, Removed }`;
  `Event::ToolStarted { …, writes: Vec<String>, texts: Vec<EditText> }` (empty for a tool with no text).
- Claude `fn tool_texts(name: &str, input: &Value) -> Vec<EditText>`: `Edit` → `new_string` empty ? `Removed` :
  `Inserted(new_string)` (`replace_all` changes nothing: first match); `MultiEdit` → one per `edits[]` item, same
  rule; `Write` → `[WholeFile]`; `NotebookEdit` and every other tool → empty.

- [x] **Step 1: Write the failing tests** (`claude.rs`): `tool_texts_of_edit_multiedit_write_and_notebook`;
  `claude_tools_fixture_edits_carry_their_new_text` (`claude-tools.jsonl`: its `Edit` decodes with `texts ==
  [Inserted("beta")]`); (`agy.rs`) `agy_edits_carry_no_text` (`agy-tools.jsonl`: the `edit_file` step's
  `ToolStarted` has `writes` and empty `texts`).
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.**
- [x] **Step 4: Run** — PASS; the gate.
- [x] **Step 4b (only if Task 0 found the new text in an agy recording):** agy gets its own `tool_texts` from those
  fields, `agy_edits_carry_no_text` becomes `agy_edits_carry_their_new_text`, and Task 8's light way then serves only
  `Removed`.
- [x] **Step 5: Commit** `M11ag Task 6: edits carry their new text`.

### Task 7: The reason, caught in the agent tab

**Files:**
- Create: `src/edit_notes.rs` (`mod edit_notes;` in `main.rs`): `REASON_MAX`, `reason_text`, `EditNote`
- Modify: `src/agent_tab.rs` (`Reasons`, `edit_events`, `AgentTabEvent::EditNoted`, `on_events`), the Workspace's
  `on_agent_event` (a no-op arm until Task 8), `src/ui_shots/agent.rs`

**Interfaces:**
- Produces (`edit_notes`): `pub const REASON_MAX: usize = 2000;` `pub fn reason_text(raw: &str) ->
  Option<Arc<str>>` (whitespace runs → one space, trimmed, cut at 2,000 chars on a char boundary; None if empty);
  `#[derive(Clone, Debug, PartialEq)] pub struct EditNote { pub reason: Arc<str>, pub reason_id: u64, pub texts:
  Vec<EditText> }`.
- Produces (`agent_tab`): `#[derive(Default)] pub(crate) struct Reasons` (the turn's last text per speaker —
  `parent: Option<String>` → `(id, Arc<str>)` — and the notes of edits started and not finished, by call id);
  `fn edit_events(event: &Event, cwd: &Path, home: Option<&Path>, open: &mut HashSet<String>, reasons: &mut
  Reasons) -> Vec<AgentTabEvent>`: `MessageDone` sets its speaker's text (a new id); `TurnDone` clears the texts;
  `ToolStarted` with paths and a text for its `parent` keeps an `EditNote`; `ToolFinished` ok → `EditDone` then
  `AgentTabEvent::EditNoted { paths: Vec<PathBuf>, note: EditNote }`; not ok → `EditDone` only.
- Live only: `on_events` is the only caller; history goes through `booted` (it applies `Past` straight to the
  conversation), so a restored conversation gives no notes.

- [x] **Step 1: Write the failing tests** (`agent_tab.rs` / `edit_notes.rs`): `a_reason_is_the_last_text_before_the_edit`
  (Claude order: `MessageDone "reuse an open tab"`, `ToolStarted Edit`, `ToolFinished ok` → one `EditNoted` with
  that reason); `an_earlier_text_in_the_turn_counts` (text, a Bash tool, then the edit); `an_edit_with_no_text_before_it_in_the_turn_gets_no_note`
  (text, `TurnDone`, edit); `a_sub_agent_edit_takes_its_own_text` (`parent: Some("t1")`); `a_failed_edit_gives_no_note`;
  `reason_text_collapses_whitespace_and_caps_on_a_char_boundary` (3,000 × `é` → 2,000 chars; `"a \n\n  b"` →
  `"a b"`; `"   "` → None); the existing `edit_events` tests keep passing.
- [x] **Step 2: Write the failing scene** `agent-edit-reason-live-only` (agent.rs area, State): a scene tab fed
  (`shot_feed`) a text and an `Edit` → one `EditNoted` heard; another fed the same as history (`shot_history`) →
  none.
- [x] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; `--changed master`.
- [x] **Step 6: Commit** `M11ag Task 7: the agent's reason for an edit`.

### Task 8: The note store, filled by the Workspace

**Files:**
- Modify: `src/edit_notes.rs` (store, anchors, light reasons), `src/workspace_agent.rs` (`EditNoted` →
  `store_notes`), `src/main.rs` (`cx.set_global(NoteStore::default())` beside the ledger's)

**Interfaces:**
- Produces:
  - `pub const PER_FILE: usize = 200; pub const FILES: usize = 2000;`
  - `#[derive(Clone, Debug, PartialEq)] pub struct Note { pub id: u64, pub agent: AgentMark, pub clock: Arc<str>,
    pub reason: Arc<str>, pub reason_key: Arc<str>, pub anchor: Arc<Vec<String>>, pub hint: usize }`
    (`reason_key` = `"a{slot}:{reason_id}"`; `hint` 0-based)
  - `#[derive(Clone, Debug)] pub struct Light { pub agent: AgentMark, pub clock: Arc<str>, pub reason: Arc<str>,
    pub reason_key: Arc<str>, pub at: Instant }`
  - `#[derive(Default)] pub struct NoteStore` (`impl Global`): `pub fn add(&mut self, path: PathBuf, note: Note)`
    (ids from the store; over `PER_FILE` the oldest note of that file goes; over `FILES` the file added to least
    recently goes); `pub fn notes(&self, path: &Path) -> Arc<Vec<Note>>` (oldest first); `pub fn generation(&self,
    path: &Path) -> u64` (bumped by each add to that path); `pub fn expect_light(&mut self, path: PathBuf, light:
    Light)`; `pub fn take_light(&mut self, path: &Path, agent: &str, now: Instant) -> Option<Light>` (the newest not
    older than `edit_ledger::KEEP`; every pending one of that path and agent is removed)
  - `pub fn update(cx: &mut App, f: impl FnOnce(&mut NoteStore))` (as `edit_ledger::update`)
  - `pub fn anchor_in(text: &str, edit: &EditText) -> Option<(Vec<String>, usize)>` (plan-time call 11: `Inserted`
    → the whole lines its first occurrence spans, without `\r`, and its first line; `WholeFile` → the first line
    and 0, None for an empty file; `Removed` → None)
  - `Workspace::store_notes(&mut self, tab: TabId, paths: Vec<PathBuf>, note: EditNote, cx)`: the agent's mark
    (`agent_ledger_who`), the clock (plan-time call 19), then off the UI thread each path is read (skipped over
    `highlight::REFUSE_ABOVE`); each text's anchor becomes a `Note`; a path whose texts give no anchor because they
    are `Removed`, or that has no texts at all (agy), gets `expect_light`; an `Inserted` text not found gives
    nothing.

- [x] **Step 1: Write the failing tests** (`edit_notes.rs`): `store_caps_per_file_and_files_oldest_first` (201st
  note drops the first; the 2,001st file drops the least recently added); `generation_counts_adds_per_path`;
  `anchor_of_an_edit_is_its_whole_lines` (new text `"self.show(tab);\n    }"` inside lines 44–45 → those two whole
  lines, hint 43); `anchor_of_a_write_is_line_one`; `replace_all_and_repeats_anchor_the_first_match`;
  `removed_has_no_anchor`; `light_reasons_latest_wins_and_expire_after_keep` (two pending, the newer taken, both
  gone; one older than 600 s → None; another agent's untouched).
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.**
- [x] **Step 4: Run** — PASS; the gate.
- [x] **Step 5: Commit** `M11ag Task 8: the note store`.

### Task 9: Notes find their lines in the file tab

**Files:**
- Modify: `src/edit_notes.rs` (`find_anchor`, `Placed`, `place`), `src/live_lines.rs` (`deleted_edges`),
  `src/file_tab.rs` (fields, `observe_global::<NoteStore>` in `new`, `place_notes` after a new text in `loaded`,
  the light claim in `live_saw` and after the settle / revise check), `src/ui_shots/notes.rs` (new area),
  `src/ui_shots/mod.rs`, `src/ui_shots/registry.rs`

**Interfaces:**
- Produces:
  - `pub fn find_anchor(lines: &[&str], anchor: &[String], hint: usize) -> Option<Range<usize>>`: every start `s`
    where `lines[s..s + anchor.len()]` equals the anchor; the one with the smallest `|s - hint|`, a tie to the
    smaller `s`.
  - `#[derive(Clone, Debug, PartialEq)] pub struct Placed { pub note: Note, pub under: usize }` (0-based line the
    note sits under: the anchor's last line)
  - `pub fn place(notes: &[Note], lines: &[&str], deadline: Instant) -> Option<Vec<Placed>>` (None past the
    deadline). Rules, newest first: a note whose anchor isn't found is skipped (kept in the store); one whose lines
    overlap a newer placed note's lines is hidden; then per `reason_key` only the topmost stays; sorted by `under`.
  - `pub fn deleted_edges(c: &Compared) -> Vec<usize>` (new-text line indices where old lines were removed with no
    changed block at or touching them)
  - `FileTab`: `notes: Rc<Vec<Placed>>`, `notes_gen: u64` (bumped when `notes` changes), `fn place_notes(&mut self,
    cx)` (off the UI thread, `live_lines::LIMIT` deadline; not for a `plain` text; the answer kept only if the doc
    version and store generation are still the ones asked with); `fn claim_light_note(&mut self, cx)` (plan-time call
    13: on a light reason taken, `NoteStore::add` a note anchored at the latest change's first block, or the line
    above the first `deleted_edges` edge); ui-shots `placed_notes() -> Vec<(usize, String)>` (under, reason).
- Keyed by the canonical path (`Live.real`), as the ledger is.

- [x] **Step 1: Write the failing tests** (`edit_notes.rs`, `live_lines.rs`): `the_anchor_nearest_the_hint_wins`
  (anchor at 10 and 50, hint 47 → 50; hint 30 → 10, tie at 30 for 10 and 50 → 10); `a_gone_anchor_is_skipped_and_comes_back`
  (place without the text → none; with it again → placed); `a_newer_overlapping_note_hides_the_older`;
  `one_note_per_reason_under_the_topmost_block`; `place_gives_up_past_the_deadline`; `deleted_edges_of_a_pure_deletion`
  (and none for a replace).
- [x] **Step 2: Write the failing scenes** (`notes.rs`, State):
  - `notes-follow-edit` ("a note following a later edit"): a file tab on a made-up `src/workspace.rs`, a note in the
    store under lines 43–45 → `placed_notes()` has it under 44 (0-based); the file rewritten with 3 lines inserted
    at the top → under 47; the anchor's text changed → no note; changed back (an undo) → under 47 again; a second
    identical copy of the block 40 lines down doesn't take it.
  - `notes-light`: a shown file tab; the ledger records an agy edit to the file (finished ok) and the store expects
    a light reason; the file changes on disk (two lines replaced) → after the read, the note sits under the
    replaced block's last line with agy's reason; a pure deletion by claude with a `Removed` text → the note sits
    under the line above the gap.
  - `notes-late`: the file tab reads the edit's change before the note is filed; filing it afterwards (the store's
    generation moves) places it without another read.
- [x] **Step 3: Run** — FAIL. **Step 4: Implement** (plan-time calls 12, 13, 14).
- [x] **Step 5: Run** — PASS; the gate; `--changed master`.
- [x] **Step 6: Commit** `M11ag Task 9: notes find their lines`.

### Task 10: Note rows in the row map (pure, and every consumer)

**Files:**
- Modify: `src/row_map.rs` (`Row::Note`, `NoteSpan`, `RowMap::file` / `RowMap::diff` take notes,
  `hidden_after`, new helpers, tests), `src/file_tab.rs` (every `match` on `Row` and every use of `Row::line()` /
  `Row::half()` / `row_at` that can meet a note row: `rescroll_row`, `bar_at_rows`, `paint_body`'s row loop (an empty
  arm until Task 11), `diff_top`, `mouse_down`, `pos_at`, `diff_row_under`, `sticky_title_at`, `place_of`,
  `change_items` / `file_change_items`, `reveal_top` callers; `row_map_of` passes `&[]`)

**Interfaces:**
- Produces: `Row::Note { note: usize, k: usize }` (`k`: the row within the note); `#[derive(Clone, Copy, Debug,
  PartialEq, Eq)] pub struct NoteSpan { pub under: usize, pub rows: usize, pub note: usize }` (sorted by `under`;
  `under` a doc line: a file line, or in Diff a diff row); `RowMap::file(widths, columns, hidden, notes:
  &[NoteSpan], row_h)`, `RowMap::diff(widths, columns, layout, notes: &[NoteSpan], row_h)`; `pub fn note_rows(&self,
  note: usize) -> Option<Range<usize>>`; `pub fn line_of(&self, r: usize) -> Option<usize>` (a note row: its
  `under`; else `row.line()`).
- Rules: a note's rows follow its `under` line's last row (a side-by-side pair: after the pair), each `row_h` tall;
  none when `under` is hidden (a fold, a folded stretch, a closed card); `Row::line()` / `half()` are None for a
  note row; `first_row` / `row_holding` / `lines_held_at` unchanged (Follow and reveals land on lines);
  `hidden_after` and `nearest_on` step over note rows; with any note the map is `Listed`.
- In `file_tab.rs`: a press or drag on a note row acts as the end of its `under` line; the sticky line and the
  breadcrumb take `line_of`; change bars and the scrollbar lane give note rows nothing; a note row is never a title.

- [x] **Step 1: Write the failing tests** (`row_map.rs`): `note_rows_follow_their_line` (lines 0–9, a 2-row note
  under 4 → rows 5, 6 are `Note{0,0}`, `Note{0,1}`, line 5 at row 7; `top(7) == 7 * row_h`);
  `a_note_under_a_hidden_line_has_no_rows`; `a_note_under_a_fold_line_follows_it_and_hidden_after_skips_it`;
  `note_rows_follow_a_wrapped_line`; `diff_note_rows_follow_the_row_and_the_pair` (unified and side by side);
  `row_at_and_rows_in_count_note_rows`; `first_row_is_unchanged_by_notes` (Follow);
  (`file_tab.rs`) `rescroll_keeps_the_line_when_a_note_row_was_on_top`; `a_press_on_a_note_row_is_its_line_s_end`
  (via `pos_at`'s pure part, or a helper it calls).
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** Every existing test passes unchanged (notes empty).
- [x] **Step 4: Run** — PASS; the gate; `--changed master` (no scene may change: no notes are passed yet).
- [x] **Step 5: Commit** `M11ag Task 10: note rows in the row map`.

### Task 11: Drawing note rows (File and Diff)

**Files:**
- Create: `src/file_tab_notes.rs` (`#[path = "file_tab_notes.rs"] mod notes;` in `file_tab.rs`)
- Modify: `src/row_map.rs` (`MapKey` gains `notes: u64`, `note_width: Option<u32>`), `src/file_tab.rs` (the
  prepaint builds the spans and the map with them; `paint_body` paints `Row::Note`; `notes_open: HashSet<u64>`;
  the "more" / "less" hit; `Frame` carries the placed notes), `src/ui_shots/notes.rs`

**Interfaces:**
- Consumes: Task 9's `notes`, `notes_gen`; Task 10's `Row::Note`, `NoteSpan`.
- Produces: `fn note_spans(placed: &[Placed], shaped: &[usize] /* wrapped line counts */, open: &HashSet<u64>,
  under_row: impl Fn(usize) -> Option<usize>) -> Vec<NoteSpan>` (pure; plan-time calls 15, 16);
  `fn note_prefix(note: &Note) -> String` (`"claude · 10:42 — "`); a shaping cache keyed by (`notes_gen`, width,
  theme) holding each note's `WrappedLine`; ui-shots probes `note_rows_drawn() -> Vec<(u64, Range<usize>)>`,
  `note_more_bounds(id) -> Option<Bounds<Pixels>>`.
- Look (spec 8): no line number; a 2 pt bar at the row's left (the gutter's mark column) in the agent's mark colour
  (as `mark_look` colours an exact mark) at `MARK_MIN`; the prefix in `text_dim`, the reason in `text`, UI font
  `text::UI`, from the code's text column, wrapped to the visible text width, painted with line height = the row
  height, clipped to the note's rows; "more" / "less" in `text_dim` at the right end of its last shown row.
  Side by side: full width under the pair. Wrap on: the same width rule.

- [x] **Step 1: Write the failing tests:** `note_spans_fold_to_two_rows_and_open_to_all` (n = 5 → 2, open → 5; n = 1
  → 1, no "more"); `note_spans_in_diff_follow_the_new_line` (`under` 43 → the row with `new == Some(44)`; none →
  no span); `note_prefix_is_agent_clock_dash`.
- [x] **Step 2: Write the failing scenes** (`notes.rs`, `Both`, Visual):
  - `notes-row`: a note (reason ~3 lines long at the tab's width, with a raw `**bold**`) under line 45 → two rows
    after row 45 (line 46's top moved down by two rows); no number in the gutter on them; the bar's pixel in the
    claude colour; the prefix's pixels dim; a selection dragged from 44 to 47 copies lines 44–47 only (no note
    words); Find for a word only in the note finds nothing; ⌥↓ from line 40 stops at the change, not on the note.
  - `notes-open`: "more" → all rows (`note_rows_drawn` grows to n), "less" → 2; the scroll keeps line 40 at the top.
  - `notes-under-fold`: fold the block holding line 45 → the note's rows gone; unfold → back.
  - `notes-diff`: unified → the note under the new line's row; side by side → across the full width under the pair;
    Wrap on → the note wraps at the visible width (more rows than with Wrap off at a narrow width).
  - `notes-follow-scroll` (State): Follow on, an agent change above a note → the view scrolls to the change's line,
    the note row not at the top.
- [x] **Step 3: Run** — FAIL. **Step 4: Implement.**
- [x] **Step 5: Run** — PASS; the gate; `--changed master`.
- [x] **Step 6: Commit** `M11ag Task 11: note rows drawn`.

### Task 12: The notes pill and the eye

**Files:**
- Modify: `src/file_tab_notes.rs` (pill, eye, next note), `src/file_tab.rs` (the bar places the pill beside Follow;
  `notes_hidden: bool`, not saved), `src/ui_shots/notes.rs`

**Interfaces:**
- Produces: `pub fn notes_pill_words(names: &[&str]) -> Option<String>` (one entry per placed note: `[]` → None;
  `["claude"]` → `"1 note from claude"`; `["claude","claude"]` → `"2 notes from claude"`; `["claude","agy"]` →
  `"2 notes"`); `pub fn next_note(note_tops: &[f32], view_bottom: f32) -> Option<usize>` (plan-time call 17);
  `fn notes_pill(&self, ui: UiColors, cx) -> Option<AnyElement>`; ui-shots `notes_pill() -> Option<(String,
  bool)>` (words, hidden), `notes_eye_bounds()`.
- The eye hides or shows every note row of this tab (the map is rebuilt: the notes generation moves); a new tab
  shows notes.

- [x] **Step 1: Write the failing tests:** `notes_pill_words_one_two_both`; `next_note_below_the_view_else_the_first`.
- [x] **Step 2: Write the failing scenes** (`notes.rs`, `Both`): `notes-pill` (three notes, two below the view: the
  pill reads `"3 notes from claude"` beside Follow; a click scrolls the first note below the view to the top
  (`reveal_top`); clicks walk on and wrap to the first; no notes → no pill; in Preview → no pill);
  `notes-hidden` (the eye → no note rows, the code rows back in place, the pill still shown and its tooltip
  `"Show notes"`; again → back; another tab on the same file still shows them).
- [x] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; `--changed master`.
- [x] **Step 6: Commit** `M11ag Task 12: the notes pill`.

### Task 13: Hand-off — docs, the full run and the on-screen check

**Files:**
- Modify: `docs/04-Development/87 - Milestone 11ag - Ask the Agent.md` ("What was built", "Calls made while
  building" — this plan's plan-time calls and any made in the build —, "Limits", "Final numbers", "On-screen
  check", status), this plan (status table, ticks, "Tasks done"), `docs/04-Development/40 - Roadmap.md` (M11ag's
  line), `docs/06-Progress-Log/60 - Progress Log.md` (a new entry), module docs of `row_map.rs` (note rows) and
  `file_tab.rs` (the bar and notes)

- [x] **Step 1: M11p grep:** `git diff master...HEAD` for added `timer(`, `request_animation_frame`, `spawn(` —
  each found is a one-shot or a background job, listed in note 87.
- [x] **Step 2: Public-repo grep** of the branch's added lines for `/Users/`, personal names and time-zone names:
  none.
- [x] **Step 3: The full `scripts/ui-shots.sh`** (before the final review); numbers into note 87. After the final
  review's fix wave, the full run again.
- [x] **Step 4: Write note 87's sections.** Limits to state at least: an `Edit` whose new text also appears earlier
  in the file anchors to the first copy; agy and pure deletions get notes only through a shown tab; notes are gone
  after a restart; no keyboard selection, so the bar follows the mouse only. **On-screen check** (~12 items, run
  with `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`, debug only, on fuxx-cli's own files such as
  `src/workspace.rs`): the bar below and above; Ask into the last-used tab (⌘↵ and the button), the box focused and
  nothing sent; Ask with no agent tab (a new tab, chip waiting); Copy with path pasted into a text editor; Diff mode
  unified and side by side; an SSH workspace (Copy only); ask Claude to edit a file and read the note under the
  change (folded, more / less); the note after a later edit; the pill and the eye; a fold over a note; idle CPU with
  a file tab showing notes (`top -l 6 -s 5 -pid <pid>`, branch vs master).
- [x] **Step 5:** Status everywhere: "🟨 built on branch m11ag-ask-agent, waiting for your on-screen check; not
  merged". The gate passes.
- [x] **Step 6: Commit** `M11ag Task 13: docs and hand-off` (explicit paths).

## Self-review (done at plan time)

- Spec coverage: 1 the bar (T4; Diff T5; SSH and agent name T3/T4); 2 Ask and ⌘↵, nothing sent, × and dedupe (T2,
  T3, T4); 3 which agent tab, last used, failed skipped, none → new tab, chip waits (T3); 4 the chip, label, hover,
  text, whole lines, path, language, cap (T1, T2, T4); 5 Copy with path (T1, T4); 6 Diff mode (T5); 7 the reason
  (T7); 7b lines from the edit, `replace_all`, `Write`, deletions, notebooks, agy (T0, T6, T8, T9); 7c the store and
  its caps (T8); 8 the note row (T10, T11); 9 the pill and eye (T12); 10 notes find their lines, gone and back,
  overlap, off the UI thread, compare limits (T9); 11 selection, Copy, Ask, Find, ⌥↑/⌥↓, folding, sticky,
  breadcrumb, scrollbar, Diff, Wrap, Follow (T10, T11). "Calls made": focus moves even in the same pane (T3), whole
  lines (T1), 500 / 64 KB cap and none for Copy (T1, T4), one chip for the same lines (T2), agent-folder path for
  Ask and M11ai form for Copy (T1, T2, T4), same-turn live reasons ≤ 2,000 plain (T7), per-tab unsaved switch (T12),
  session store 200 / 2,000 (T8), `Write` line 1 / `replace_all` first match / deletion light / no notebooks (T6,
  T8, T9), a gone note comes back (T9), last-used agent else Claude and SSH Copy only (T3, T4), clock time (T8, T11).
  Left to the builder: every listed scene has a task (bar below / above T4, Ask into an existing tab and with none
  T3, Diff T5, folded / opened T11, pill T12, hidden T12, following a later edit T9, under a fold T11).
- Names checked across tasks: `CodeRef`, `CodeLines`, `AskTo`, `file_ref`, `diff_ref`, `agent_path`, `copy_text`,
  `shown_path`, `Attachment::Code`, `wire_text(.., cwd)`, `pick_agent_tab`, `ask_agent`, `ask_code`, `set_ask`,
  `tell_ask_targets`, `pending_chips`, `FileTabEvent::Ask`, `AskAgent`, `sel_bar_y`, `EditText`, `tool_texts`,
  `Reasons`, `EditNote`, `EditNoted`, `reason_text`, `Note`, `Light`, `NoteStore`, `anchor_in`, `store_notes`,
  `find_anchor`, `Placed`, `place`, `deleted_edges`, `place_notes`, `claim_light_note`, `Row::Note`, `NoteSpan`,
  `note_rows`, `line_of`, `note_spans`, `notes_pill_words`, `next_note`.
