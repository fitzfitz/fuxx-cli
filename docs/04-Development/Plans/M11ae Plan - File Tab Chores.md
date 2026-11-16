---
tags: [development, plan, milestone]
milestone: M11ae
status: planned
updated: 2026-11-16
---

# M11ae Plan — File tab chores

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[80 - Milestone 11ae - File Tab Chores]] |
| **Status** | 📋 Planned 2026-11-16 |
| **Tasks done** | 0 / 6 |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | Yours: option B, overlay scrollbar, "yes, go ahead". The numbers in note 80 are the AI's calls, approved with the design. |
| **Needs your eyes** | 👀 the on-screen check in note 80, after Task 6 |

**Goal:** A file tab gets the option-B title bar, an overlay scrollbar in File, Diff and Preview, and drag-select that
scrolls past the text's edge.

**Architecture:** Pure helpers first (change counts from the existing `-U0` diff, the header's words, scrollbar
geometry, auto-scroll speed), each unit-tested in `cargo test`. The UI goes in `src/file_tab.rs`; the scrollbar's
geometry, fade state and painting live in a new `src/scrollbar.rs` so the text canvas and the Preview list share them.
Everything visible that changes over time (the age, the fade, the auto-scroll) is set from timers or events followed by
`cx.notify()`, never inside `render` (M11p).

**Tech Stack:** Rust, GPUI (pinned git rev `1d2a4b3f…`: `ListState::scroll_px_offset_for_scrollbar`,
`max_offset_for_scrollbar`, `set_offset_from_scrollbar`, `viewport_bounds`, `set_scroll_handler`,
`Context::spawn_in`), syntect (already used by `src/highlight.rs`). **No new crates.**

**Spec:** [[80 - Milestone 11ae - File Tab Chores]]

## Global Constraints

- Work in worktree `.worktrees/m11ae-file-tab`, branch `m11ae-file-tab` from master `8d2b16f`. Commit per task;
  merging is the user's call.
- **Gate per task:** `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features
  ui-shots && cargo fmt --check`, then the task's scenes, then the full `cargo run --features ui-shots -- __ui-shots`
  once before the commit. No new clippy warnings over the baselines Task 1 records.
- **M11p:** nothing visible changes synchronously inside any `render`; timers and handlers set state then `cx.notify()`.
  No timer runs while nothing is happening (the fade and auto-scroll tasks end themselves).
- Copy and numbers exactly as note 80: bar 42 pt; chip `M · +3 −1` / `A · +12` (` · ` is U+00B7 with spaces, `−` is
  U+2212); meta joined by ` · `; folders by ` / `; `Plain Text`; `1 line` / `42 lines` / `12,345 lines`; ages
  `edited just now`, `edited N min ago`, `edited N h ago`, `edited N d ago`, `edited N mo ago`, `edited N y ago`;
  thumb 6 pt, 3 pt corners, 3 pt inset, min 24 pt, 35 % / 55 % opacity of `ui.text`, zone 12 pt, hide after 1.0 s,
  fade 200 ms; auto-scroll every 16 ms by `(distance / 8).clamp(1, 3 × line height)` pt.
- Tooltips: `Open in Editor`, Wrap's and Preview's as today (`TIP_WRAP`, `TIP_PREVIEW`, `TIP_PREVIEW_TOO_LARGE`),
  `TIP_FILE`, `TIP_DIFF`.
- Rendering behaviour is checked in ui-shots scenes (GPUI test contexts exist only with `--features ui-shots`); pure
  logic in `cargo test`.

## Review Focus

1. **A file that changes on disk while shown:** the chip's counts, the line count and the age follow the new version
   (counts arrive with the next change look-up, as the bars do). Task 1 tests counts are taken with the bars'
   version check; Task 2's scene reloads a file and reads the header's words again.
2. **Switching mode or Wrap while the scrollbar shows or a drag-select runs:** the thumb is re-measured from the new
   geometry, and the auto-scroll stops on a mode switch (`set_mode` already clears the selection). Task 5 tests a mode
   switch mid-drag stops the task.
3. **A text that fits (or an empty file):** no thumb, a press in the edge zone selects as before, no division by zero.
   Task 3's geometry tests cover `content <= viewport` and `viewport == 0`.
4. **A pointer far outside (another monitor, a 2,000 pt drag):** the speed caps at 3 lines a step and the scroll stays
   within `max_scroll`. Task 5's speed and clamp tests.
5. **Idle CPU:** a shown tab that isn't touched runs only its existing 1 s poll; after the fade nothing repaints.
   Task 3's scene checks the fade task has ended (`scrollbar_animating() == false`) 1.5 s after the last scroll.

---

### Task 1: Header facts — change counts, type, where, lines, age

**Files:**
- Modify: `src/git_status.rs` (`line_changes` at `:270`, `parse_hunks` at `:199`, `all_added` at `:342`)
- Modify: `src/highlight.rs` (`Highlighter` at `:40`; the syntax lookup in `highlight` at `:73-89`)
- Modify: `src/file_tab.rs` (`Doc` at `:538`, `accept_changes` at `:634`, the look-up at `:2296-2372`)

**Interfaces:**
- Produces (git_status):
  - `pub struct LineChanges { pub lines: Vec<(usize, LineChange)>, pub added: usize, pub removed: usize, pub new: bool }`
  - `pub fn hunk_counts(diff: &str) -> (usize, usize)` — (added, removed): Σd and Σb over `@@ -a,b +c,d @@` headers
    (a missing count is 1).
  - `line_changes(...) -> Option<LineChanges>` (was `Option<Vec<…>>`); untracked / no HEAD: `new: true`,
    `added` = the line count `all_added` finds, `removed: 0`.
- Produces (highlight): `pub fn syntax_name(&self, path: &Path, first_line: &str) -> String` — the same lookup as
  `highlight` (extension, file name, first line); `"Plain Text"` when none. `highlight` uses it (one lookup, DRY).
- Produces (file_tab):
  - `Doc` gains `pub counts: Option<(usize, usize, bool)>` (added, removed, new), set by `accept_changes` with the
    same version check as the bars; `Doc` gains `pub syntax: Option<Arc<str>>`, set where spans are accepted.
  - `pub fn chip_label(counts: Option<(usize, usize, bool)>) -> Option<String>`
  - `pub fn where_label(path: &Path, repo: Option<(&Path, &Path)>, home: Option<&Path>) -> String` — `repo` is
    `(root, rel)` from `GitPlace`.
  - `pub fn lines_label(n: usize) -> String` (uses the existing `thousands`)
  - `pub fn age_label(modified: SystemTime, now: SystemTime) -> String`

- [ ] **Step 0: Record the clippy baselines** (`cargo clippy --all-targets 2>&1 | grep -c '^warning'`, and with
  `--features ui-shots`) in the task report.
- [ ] **Step 1: Write the failing tests**

```rust
// git_status
#[test] fn hunk_counts_add_new_and_old_sides() {
    assert_eq!(hunk_counts("@@ -3,0 +4,2 @@\n@@ -9 +10,0 @@\n@@ -7 +7 @@ x\n"), (3, 2));
    assert_eq!(hunk_counts(""), (0, 0));
}
// extend the existing line_changes repo tests: a changed file → added/removed match `git diff --numstat`,
// new == false; an untracked 3-line file → (3, 0, new == true).

// file_tab
#[test] fn the_chip_says_modified_or_added_with_its_counts() {
    assert_eq!(chip_label(Some((3, 1, false))).as_deref(), Some("M \u{b7} +3 \u{2212}1"));
    assert_eq!(chip_label(Some((3, 0, false))).as_deref(), Some("M \u{b7} +3"));
    assert_eq!(chip_label(Some((0, 2, false))).as_deref(), Some("M \u{b7} \u{2212}2"));
    assert_eq!(chip_label(Some((12, 0, true))).as_deref(), Some("A \u{b7} +12"));
    assert_eq!(chip_label(Some((0, 0, false))), None);
    assert_eq!(chip_label(None), None);
}
#[test] fn where_starts_at_the_repo_or_home() {
    let (root, home) = (Path::new("/w/fuxx-cli"), Path::new("/Users/me"));
    assert_eq!(where_label(Path::new("/w/fuxx-cli/src/ui_shots/README.md"),
        Some((root, Path::new("src/ui_shots/README.md"))), Some(home)), "fuxx-cli / src / ui_shots");
    assert_eq!(where_label(Path::new("/w/fuxx-cli/a.rs"), Some((root, Path::new("a.rs"))), Some(home)), "fuxx-cli");
    assert_eq!(where_label(Path::new("/Users/me/Downloads/x.txt"), None, Some(home)), "~/Downloads");
    assert_eq!(where_label(Path::new("/etc/hosts"), None, Some(home)), "/etc");
}
#[test] fn lines_and_ages_read_as_note_80_says() {
    assert_eq!(lines_label(1), "1 line"); assert_eq!(lines_label(42), "42 lines");
    assert_eq!(lines_label(12345), "12,345 lines");
    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    let ago = |s| age_label(t - Duration::from_secs(s), t);
    assert_eq!(ago(59), "edited just now"); assert_eq!(ago(60), "edited 1 min ago");
    assert_eq!(ago(3_599), "edited 59 min ago"); assert_eq!(ago(3_600), "edited 1 h ago");
    assert_eq!(ago(86_400), "edited 1 d ago"); assert_eq!(ago(29 * 86_400), "edited 29 d ago");
    assert_eq!(ago(30 * 86_400), "edited 1 mo ago"); assert_eq!(ago(365 * 86_400), "edited 1 y ago");
    assert_eq!(age_label(t + Duration::from_secs(5), t), "edited just now");
}
#[test] fn counts_for_an_older_version_are_dropped() { /* as a_highlight_for_an_older_version_is_dropped */ }
// highlight: syntax_name("a.rs", "") == "Rust"; ("Makefile", "") == "Makefile"; ("x", "#!/bin/bash") == "Bourne Again Shell (bash)"; ("x.zzz", "") == "Plain Text"
```

- [ ] **Step 2: Run them to see them fail** — `cargo test --bin fuxx hunk_counts chip_label where_ lines_and_ages syntax_name` → compile errors / FAIL.
- [ ] **Step 3: Implement.** Months are 30 days, years 365; `where_label` outside a repo shortens a path under `home`
  to `~/…` (the home folder itself is `~`). Callers of `line_changes` (`file_tab.rs:2348` and the tests) move to
  `LineChanges`. `colour_text` sets `doc.syntax` from the shared `Highlighter` when it accepts spans.
- [ ] **Step 4: Run the gate** — all tests pass; clippy counts at the baselines.
- [ ] **Step 5: Commit** — `M11ae Task 1: header facts (change counts, type, where, lines, age)`.

### Task 2: The option-B bar — title, details, one view switch, icon buttons

**Files:**
- Modify: `src/file_tab.rs` (`toolbar` `:3254`, `toggle` `:3237`, `mode_segment` `:3093`, `wrap_button` `:3154`,
  `preview_button` `:3189`, `tick` `:2013`, `shows_preview` `:1107`, `toggle_bounds` `:3674`)
- Modify: `src/tab_bar.rs` (`Glyph` `:973`, its painter near `:1115`)
- Modify: `src/ui_shots.rs` (scenes using `toggle_bounds()` / `preview_bounds()`: `file-diff-toggle*`,
  `file-tab-md-*`), `src/ui_shots/chrome_sources.rs` (`chrome_file_tab_toolbar` `:1846`)

**Interfaces:**
- Consumes: Task 1's `chip_label`, `where_label`, `lines_label`, `age_label`, `Doc::counts`, `Doc::syntax`.
- Produces:
  - `pub enum View { File, Diff, Preview }`; `pub fn view_segments(in_repo: bool, mode: FileMode, markdown: bool) -> Vec<View>`
    (File always; Diff when `shows_toggle(in_repo, mode)`; Preview when `markdown`); the switch shows when `len() >= 2`.
  - `pub fn current_view(mode: FileMode, preview_on_and_fits: bool) -> View`
  - `toggle_bounds(&self) -> [Option<Bounds<Pixels>>; 3]` (File, Diff, Preview); `preview_bounds` returns the Preview
    segment's.
  - `FileTab::age: Option<String>`, refreshed in `tick` (`refresh_age(&mut self, now: SystemTime, cx)` notifies only
    when the words change); `pub fn header_words(&self) -> (String, Option<String>, String)` (ui-shots: name, chip,
    meta line) for scenes.
  - `Glyph::Wrap`, `Glyph::OpenExternal` (12 pt box, drawn as note 80 says).

- [ ] **Step 1: Write the failing tests** — `cargo test`:
  `view_segments(true, Plain, true) == [File, Diff, Preview]`; `(false, Plain, true) == [File, Preview]`;
  `(false, Plain, false) == [File]` (no switch); `(false, Diff, false) == [File, Diff]`;
  `current_view(Diff, true) == Diff`; `current_view(Plain, true) == Preview`; `current_view(Plain, false) == File`.
  Replace `the_toggle_shows_in_a_repo_or_in_diff_mode` and `preview_shows_for_markdown_in_file_mode_only` with these.
- [ ] **Step 2: Run them; see them fail.**
- [ ] **Step 3: Implement** the bar as note 80 §1: `FILE_TOOLBAR: f32 = 42.0` for this bar only (`TOOLBAR` stays for
  the find bar); name `text::NAME` semibold; chip `text::META` on `ui.panel` with a 1 pt `ui.panel_border`, full
  rounding, its letter in the change bar's colour (`BarColours::of_change`: `M` Changed, `A` Added); meta line
  `text::META` in `ui.text_dim`, its Where part `flex_shrink` + `truncate`, the rest `flex_none`. Segment clicks:
  File → `choose_preview(false)` then `choose_mode(Plain)`; Diff → `choose_mode(Diff)`; Preview (when it fits) →
  `choose_mode(Plain)` then `choose_preview(true)`. Wrap and Open in Editor via `icon_button` (Wrap filled with
  `ui.card_selected` when on). The age: in `tick`'s poll and when a version is loaded, from `self.stamp`'s
  `modified`.
- [ ] **Step 4: Scenes.** Update every scene that clicked the old toggle / Preview button to the 3-segment bounds.
  New `file-tab-header` (`file_tab_scenes`): a temp repo with a committed file then edited (+2 −1) → `header_words()`
  is `("a.rs", Some("M · +2 −1"), "<repo> / src · Rust · N lines · edited just now")`; an untracked file → chip
  `A · +3`; a file outside a repo → no chip, Where `~/…` or the temp folder; at a 320 pt pane width the controls'
  bounds stay inside the pane. A reload (append a line) → the line count and counts follow within the scene's
  look-up wait.
- [ ] **Step 5: Gate, full `__ui-shots`, commit** — `M11ae Task 2: the option-B file bar`.

### Task 3: The overlay scrollbar in File and Diff

**Files:**
- Create: `src/scrollbar.rs` (add `mod scrollbar;` in `src/main.rs`)
- Modify: `src/file_tab.rs` (`set_scroll` `:2710`, `wheel` `:2723`, `mouse_down` `:2782`, the body canvas `:3540-3590`)
- Modify: `src/ui_shots.rs` (new scene)

**Interfaces:**
- Produces (`src/scrollbar.rs`):
  - `pub enum Axis { Vertical, Horizontal }`
  - `pub struct Thumb { pub start: f32, pub len: f32 }` (along the axis, from the track's start)
  - `pub fn thumb(viewport: f32, content: f32, offset: f32) -> Option<Thumb>` — `None` when `content <= viewport` or
    `viewport <= 0`; `len = max(viewport² / content, 24)`, capped at `viewport`; `start` maps offset `0..=content-viewport`
    onto `0..=viewport-len`.
  - `pub fn offset_for(thumb_start: f32, viewport: f32, content: f32) -> f32` — the inverse, clamped.
  - `pub fn alpha(since_active: Duration, hot: bool) -> f32` — `hot` (hovered or dragged): 0.55; else 0.35 until
    1.0 s, then linear to 0 at 1.2 s.
  - `pub struct Bar { active_at: Option<Instant>, hot: bool, drag: Option<(Axis, f32)>, animating: bool }` with
    `poke(&mut self, now)`, `is_shown(now) -> bool`, `start_drag/end_drag`, and
    `pub fn paint(window: &mut Window, area: Bounds<Pixels>, axis: Axis, t: &Thumb, alpha: f32, color: Rgb)`
    (6 pt, 3 pt radius, 3 pt inset).
- Produces (file_tab): `FileTab::scrollbar: Bar`; `fn scrolled(&mut self, cx)` (pokes and starts the fade task:
  `cx.spawn` stepping 16 ms while `alpha > 0`, ends itself); ui-shots `scrollbar_shown() -> bool`,
  `scrollbar_animating() -> bool`, `thumb_bounds(Axis) -> Option<Bounds<Pixels>>`.

- [ ] **Step 1: Write the failing tests** (`scrollbar.rs`): `thumb(100, 100, 0) == None`; `thumb(0, 500, 0) == None`;
  `thumb(100, 400, 0) == Thumb{start:0, len:25}`; `thumb(100, 400, 300).start == 75`; `thumb(100, 100_000, 0).len == 24`;
  `offset_for(75, 100, 400) == 300`; `offset_for(-5, …) == 0`; `offset_for(999, …) == 300`;
  `alpha(0.5 s, false) == 0.35`; `alpha(1.1 s, false) ≈ 0.175`; `alpha(1.3 s, false) == 0`; `alpha(5 s, true) == 0.55`.
- [ ] **Step 2: Run; fail.**
- [ ] **Step 3: Implement.** Every change of the shown scroll goes through `set_scroll` (wheel, find, reveal, the
  auto-scroll) and calls `scrolled`. The canvas paints the thumbs after `paint_body` when `is_shown`; vertical content
  is `rows × line_height`, horizontal (Wrap off, `widest` cells wider than the text) `widest × cell_width + RIGHT_PAD`.
  Mouse moves within 12 pt of the right (or bottom) edge set `hot` and poke; leaving clears `hot`. `mouse_down` in a
  zone: on the thumb, start a drag keeping the grab offset; off it, jump (centre on the press) then drag; no selection.
  The window-wide move / up handlers already registered in the canvas paint drive the drag and end it.
- [ ] **Step 4: Scene `file-tab-scrollbar`:** a 400-line file; before any scroll `scrollbar_shown()` is false; one wheel
  step → shown, `thumb_bounds(Vertical)` inside the text's right 12 pt and its top proportional to the scroll; 1.5 s
  later not shown and `scrollbar_animating()` false; drag the thumb to the bottom → `scroll().y == geom().max_scroll.y`;
  a press below the thumb → it centres there; Wrap off with a 3,000-char line → the horizontal thumb exists; a
  10-line file → never shown, and a press in the zone starts a selection.
- [ ] **Step 5: Gate, full `__ui-shots`, commit** — `M11ae Task 3: overlay scrollbar in File and Diff`.

### Task 4: The scrollbar in Preview

**Files:**
- Modify: `src/file_tab.rs` (`preview_body` `:2532`)
- Modify: `src/ui_shots.rs` (new scene in `file_tab_md_scenes`)

**Interfaces:**
- Consumes: Task 3's `thumb`, `offset_for`, `alpha`, `Bar`, `Bar::paint`, `scrolled`.
- Uses GPUI `ListState`: `viewport_bounds()` (viewport), `max_offset_for_scrollbar().y + viewport` (content),
  `-scroll_px_offset_for_scrollbar().y` (offset), `set_offset_from_scrollbar(point(0, -offset))` (drag; verify the
  sign against `list.rs:753-783` at the pinned rev), `scrollbar_drag_started/ended`, `set_scroll_handler` (the list's
  own wheel scrolls poke the bar).

- [ ] **Step 1: Scene first, `file-tab-md-preview-scrollbar`:** a 300-paragraph Markdown file in Preview; a wheel step
  → `scrollbar_shown()`; drag the thumb to the bottom → `preview_list().is_scrolled_to_end() == Some(true)`; a short
  file → never shown. Run it; it fails.
- [ ] **Step 2: Implement:** an absolute canvas over the list paints the thumb and takes the 12 pt edge zone's presses
  (the list's own presses — links — are unchanged elsewhere).
- [ ] **Step 3: Gate, full `__ui-shots`, commit** — `M11ae Task 4: scrollbar in Preview`.

### Task 5: Drag-select past the edge scrolls

**Files:**
- Modify: `src/file_tab.rs` (`pos_at` `:2733`, `mouse_drag` `:2813`, the up handler `:3584`, `set_mode` `:2660`)
- Modify: `src/ui_shots.rs` (new scene)

**Interfaces:**
- Consumes: Task 3's `scrolled` (through `set_scroll`).
- Produces:
  - `pub fn edge_speed(outside: f32, line_height: f32) -> f32` — 0 when `outside <= 0`, else
    `(outside / 8).clamp(1, 3 × line_height)`.
  - `pub fn outside(p: f32, lo: f32, hi: f32) -> f32` — signed: `p - hi` when past `hi`, `p - lo` (negative) before
    `lo`, else 0.
  - `FileTab::drag_at: Option<Point<Pixels>>` (the last pointer place while selecting) and `autoscrolling: bool`;
    ui-shots `autoscrolling() -> bool`, `selection() -> Option<(Pos, Pos)>`.

- [ ] **Step 1: Write the failing tests:** `edge_speed(0, 18) == 0`; `edge_speed(-3, 18) == 0`;
  `edge_speed(4, 18) == 1`; `edge_speed(80, 18) == 10`; `edge_speed(2000, 18) == 54`; `outside(50, 0, 100) == 0`;
  `outside(130, 0, 100) == 30`; `outside(-20, 0, 100) == -20`.
- [ ] **Step 2: Run; fail.**
- [ ] **Step 3: Implement.** `mouse_drag` stores `drag_at` and, when the pointer is outside the text vertically (or
  horizontally with Wrap off) and no task runs, starts one with `cx.spawn_in(window, …)`: every 16 ms, while
  `selecting` and still outside, `set_scroll(scroll + (sign × edge_speed) per axis)`, then the selection head =
  `pos_at(drag_at clamped into the text bounds)` (last row/column inside); it ends itself when not selecting, back
  inside, or the scroll didn't move. `mouse_drag` itself also clamps into the bounds (no more invisible growth).
  `set_mode` and mouse up end it (they clear `selecting`).
- [ ] **Step 4: Scene `file-tab-drag-select`:** a 400-line file; press on line 3; move the pointer 60 pt below the text;
  wait 300 ms with no moves → `autoscrolling()`, `scroll().y > 0`, and the selection's head line ≥ the last line in
  sight at the start; move 300 pt below → the scroll per frame is larger; release → `autoscrolling()` false within
  50 ms and the scroll stays. Above the text after scrolling down → scrolls up. Wrap off with a long line, right of
  the text → `scroll().x` grows. A switch to Diff mid-drag → it stops.
- [ ] **Step 5: Gate, full `__ui-shots`, commit** — `M11ae Task 5: drag-select scrolls past the edge`.

### Task 6: Docs, verification, hand-off

**Files:** `docs/04-Development/80 - Milestone 11ae - File Tab Chores.md`, this plan, `docs/04-Development/40 - Roadmap.md`,
`docs/06-Progress-Log/60 - Progress Log.md`.

- [ ] **Step 1:** Full gate and full `__ui-shots` on the branch tip; record test count, clippy counts, scene count.
- [ ] **Step 2:** Note 80: status 🟨 built, a "What was built" section (any plan-time calls the tasks made), the
  on-screen checklist unchanged unless a task changed behaviour. Roadmap entry → 🟨 built, waiting for your check.
  This plan's table → 6 / 6. A Progress Log entry.
- [ ] **Step 3: Commit** — `M11ae: docs and results`. Then the whole-branch review, then ask the user to merge.
