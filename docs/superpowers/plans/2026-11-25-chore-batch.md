# Chore batch 2026-11-25 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (the repo's milestone build mode, AGENTS.md) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Thirteen chores, one task each (then docs and the review): a 2-line commit text area with greyed buttons, the clock panel centred, your message's hover icons that stay, the plan tab flush right, a track cursor that clears on a click elsewhere, folder picker A, changed-files popover A (with the duplicate-path bug fixed), and a readable Plan card for Claude's Plan Mode.

**Architecture:** Each chore reuses what exists. The commit field becomes a `Composer` capped at 2 rows. The clock panel goes through `menu_frame` with a sideways offset. The picker and the ledger popover get pure row models (unit-tested) and new drawing. The plan is a `Permission` that carries a `PlanDraft`, so waiting, expiry, withdrawal and answering all work as they do for any permission. Claude-specific reading stays in `providers/claude.rs`.

**Tech Stack:** Rust, GPUI (pinned git rev), the existing `agent_markdown` / `md_preview` renderer, git CLI.

**Spec:** `docs/superpowers/specs/2026-11-25-chore-batch-design.md`

## Global Constraints

- Branch `chore/batch-2026-11-25` off `master`. Stage files by name, never `git add -A` (other sessions edit this repo).
- Per-task gate: `cargo test --bin fuxx`, `cargo clippy --all-targets -- -D warnings`, `cargo clippy --all-targets --features ui-shots -- -D warnings`, `cargo fmt --check`. No screenshot runs per task.
- No functional text under 11 px. Sizes and colours come from `ui_tokens.rs` and the theme's `ui()` / `Colors`.
- Provider-specific code stays in `src/providers/<agent>.rs`; generic code asks through `providers/mod.rs` types.
- Context7 before using any library API you haven't already seen in this repo (AGENTS.md).
- GUI behaviour can't be unit-tested: say so, and list it under note 93's "Checklist for you".

## Review Focus

1. A plan answered and then the tab restarted or resumed: the card must show its answer (or "expired"), never live buttons for an ask Claude no longer holds. → Task 11's `a_plan_expires_with_the_turn` test.
2. A plan with no `#` heading, or an empty `plan`: the title falls back to "Plan" and the card still renders. → Task 11's `plan_title_falls_back` test.
3. Changed files outside a git repo, or in a repo with no commits yet (`HEAD` missing): no panic, rows without counts, status from existence. → Task 10's `facts_without_git` and `facts_without_head` tests.
4. Script text naming a path that is a folder, or no file at all: it never shows as a file. → Task 10's `script_named_folders_and_missing_files_are_dropped` test.
5. A commit message pasted with 5 lines: the box shows 2 rows and scrolls, and the whole message is committed. → Task 1's `commit_keeps_every_line` test.

---

### Task 1: The commit box is a 2-line text area; ✦ and Commit grey out when they can't act

**Files:**
- Modify: `src/composer.rs` (`MAX_ROWS` becomes the default of a per-box `max_rows`)
- Modify: `src/commit_line.rs` (the field becomes a `Composer`; pure `CommitButtons`; `ENTER_NOTE` goes)
- Modify: `src/changes_panel.rs` (only if it names `NameInput` for the field)

**Interfaces:**
- Produces: `Composer::set_max_rows(&mut self, rows: usize, cx: &mut Context<Self>)` (rows ≥ 1; default 8). `pub fn commit_buttons(staged: usize, message: &str, busy: bool) -> CommitButtons` with `pub struct CommitButtons { pub write: bool, pub commit: bool }`.

- [ ] **Step 1: Write the failing tests** in `commit_line.rs`'s tests:
  - `commit_buttons_follow_what_can_act`: `commit_buttons(0, "x", false) == CommitButtons { write: false, commit: false }`; `commit_buttons(2, "", false) == { write: true, commit: false }`; `commit_buttons(2, "  \n ", false) == { write: true, commit: false }`; `commit_buttons(2, "fix", false) == { write: true, commit: true }`; `commit_buttons(2, "fix", true) == { write: false, commit: false }`.
  - `commit_keeps_every_line`: `CommitState`'s draft for a repo set to `"a\nb\nc\nd\ne"` reads back unchanged. Use the existing draft API (`swap_draft`).
  - In `composer.rs`: `max_rows_caps_the_shown_rows`. Five lines of text with `max_rows = 2` show 2 rows. Test through the pure height helper the box already uses (the `rows.min(MAX_ROWS)` sites, which now read `max_rows`).
- [ ] **Step 2: Run** `cargo test --bin fuxx commit_buttons commit_keeps max_rows` → FAIL (not defined).
- [ ] **Step 3: Implement.** Add `max_rows: usize` to `Composer` (default `MAX_ROWS`) and use it at every `MAX_ROWS` use. In `prepare_commit_line`:
  - Make the field `Composer::new(field_hint(..).into(), …)` with `set_bare(true)` and `set_max_rows(2)`.
  - `ComposerEvent::Send(_)` → `commit_now` (↵ commits; ⇧↵ is the composer's own newline). `ComposerEvent::Stop` → what `NameInputEvent::Cancel` did. Ignore the other events.
  - Keep the `CommitInput` key-context wrapper, so ⌘↵ still runs `CommitStaged`.
  - Remove `ENTER_NOTE` and its tests.
  
  In `render_commit_line`, drive ✦ and Commit from `commit_buttons`. Off: the theme's faint ink, no `.hover`, no `.on_click`, default cursor; the tooltip still says why (`commit_blocked`).
- [ ] **Step 4: Run** the three tests → PASS; then the per-task gate.
- [ ] **Step 5: Commit** `src/composer.rs src/commit_line.rs` (+ `changes_panel.rs` if touched): `Chore (Task 1): the commit message is a 2-line text area; ✦ and Commit grey out when they can't act`.

### Task 2: The clock panel opens centred over its icon

**Files:** Modify `src/agent_box_menus.rs:17` (`menu_frame`), `src/agent_box.rs:1252` (the clock's call).

**Interfaces:**
- Produces: `pub(super) fn menu_frame_at(&self, content: Option<AnyElement>, side: Option<AnyElement>, width: f32, dx: f32, cx: &mut Context<Self>) -> AnyElement`. `menu_frame` calls it with `dx = 0.`. `pub(super) fn centred_dx(anchor: f32, width: f32) -> f32` = `(anchor - width) / 2.`.

- [ ] **Step 1: Failing test** `centred_dx_puts_the_panel_middle_over_the_button`: `centred_dx(28., 280.) == -126.` and `centred_dx(280., 280.) == 0.`. Run → FAIL.
- [ ] **Step 2: Implement** `menu_frame_at`: the anchored offset becomes `point(px(dx), px(-MENU_GAP))`, and `snap_to_window_with_margin` keeps it inside the window. The clock passes `centred_dx(BUTTON, CLOCK_WIDTH)`. Only the clock changes; the usage panel stays as it is.
- [ ] **Step 3: Run** → PASS; the gate.
- [ ] **Step 4: Commit**: `Chore (Task 2): the clock panel opens centred over its icon`.

### Task 3: Your message's hover icons stay under the pointer

**Files:** Modify `src/agent_rewind.rs:1099-1113` (`you_actions`' wrapper).

- [ ] **Step 1: Debug with superpowers:systematic-debugging** before changing anything. The likely cause: the wrapper is `absolute().right_full()`, outside the `YOU_ROW` group's box, so moving onto it ends the group hover and `opacity(0.)` hides it. Confirm it by comparing the wrapper's bounds with the group element's, from a ui-shots marked frame or by reading the layout.
- [ ] **Step 2: Fix the confirmed cause.** If it's the one above, give the wrapper its own `.hover(|s| s.opacity(1.))`, so it stays shown while the pointer is on it.
- [ ] **Step 3: The gate.** GUI-only: it goes on note 93's checklist.
- [ ] **Step 4: Commit**: `Chore (Task 3): your message's hover icons stay while the pointer is on them`.

### Task 4: The plan tab sits flush with the box's right edge

**Files:** Modify `src/agent_box.rs:131-135,1740-1790`. Maybe `src/icons.rs` + `assets/icons/shapes/` (a mirrored step sprite).

- [ ] **Step 1:** `PLAN_TAB_INSET` goes to 0, so the tab's right border lines up with the box's right edge.
- [ ] **Step 2:** The step goes on the plan tab's **left**: the same `paint_tab_step` sprite, mirrored on x. Add a mirrored sprite (e.g. `tab-step-fill-left.svg` / `tab-step-edge-left.svg`) to `icons.rs`'s FILES if `paint_tab_step` can't flip.
- [ ] **Step 3:** The box's top-right radius becomes 0 while the plan tab shows, as the top-left already is under the folder tab. Find where the box's `rounded_t*` is set for the folder tab and do the same on the right. `PLAN_TAB_GAP` still holds the least room between the two tabs.
- [ ] **Step 4: The gate**, then `scripts/ui-shots.sh agent-plan-tab` to check the corner. GUI-only: it goes on note 93's checklist.
- [ ] **Step 5: Commit**: `Chore (Task 4): the plan tab sits flush right, mirroring the folder tab`.

### Task 5: A click elsewhere clears a track column's cursor

**Files:** Modify `src/agent_tracks.rs:152,597-690`.

**Interfaces:** Produces `pub fn cursor_after_click(cursor: Option<TrackCursor>, on_tracks: bool, detail_open: bool) -> Option<TrackCursor>`.

- [ ] **Step 1: Failing test** `a_click_elsewhere_clears_the_cursor`: `(Some(c), false, false) → None`, `(Some(c), true, false) → Some(c)`, `(Some(c), false, true) → Some(c)`, `(None, false, false) → None`. Run → FAIL.
- [ ] **Step 2: Implement:** an `on_mouse_down_out` on the step's tracks element (`tracks()`, line 152) applies it. When the cursor clears and the tracks held the keyboard, `refocus = Some(Refocus::Box)` (as `track_leave` does).
- [ ] **Step 3: Run** → PASS; the gate.
- [ ] **Step 4: Commit**: `Chore (Task 5): a click outside the tracks clears a column's cursor`.

### Task 6: The plan popover is left-aligned

**Files:** Modify `src/agent_box.rs:1793` (`plan_tab`).

- [ ] **Step 1:** The `☰ N/M` tab's popover opens with its left edge on the tab's left edge (`menu_frame_at` from Task 2 with `dx = 0.`, anchored on the tab itself). The window snap keeps it inside.
- [ ] **Step 2: The gate**, then `scripts/ui-shots.sh agent-plan-tab`. GUI-only: it goes on note 93's checklist.
- [ ] **Step 3: Commit**: `Chore (Task 6): the plan popover opens left-aligned on its tab`.

### Task 7: Your bubble wraps in a narrow pane

**Files:** Modify `src/agent_rows.rs` (the `Item::You` row, up to line 310). Add a ui-shots scene.

- [ ] **Step 1: Debug with superpowers:systematic-debugging** first. Find which element in your message's row has no width cap or `min_w(0)`, so the text doesn't wrap.
- [ ] **Step 2: Failing scene:** an agent pane about 420 pt wide with a 120-character one-line message. Assert the bubble's bounds sit inside the pane. Run it → FAIL.
- [ ] **Step 3: Fix the cause:** the bubble's max width is the chat column's width less its side gutters, and its text wraps. Run the scene → PASS; the gate.
- [ ] **Step 4: Commit**: `Chore (Task 7): your message's bubble wraps in a narrow pane instead of running off the left`.

### Task 8: A copy icon on the chat's code blocks

**Files:** Modify `src/md_preview.rs`. Add a ui-shots scene.

**Interfaces:** Produces `pub fn code_copy_text(code: &str) -> String`.

- [ ] **Step 1: Failing test** `chat_code_copies_its_raw_text`: tabs kept, no trailing newline added; `"a\tb\n"` → `"a\tb\n"`. Run → FAIL.
- [ ] **Step 2: Implement.**
  - The chat draws with `reader: None` (`agent_tab.rs:6337`), so its code blocks skip `code_frame`'s header.
  - For `reader: None`, wrap the block `relative()` and add an absolute top-right 22 pt button with `Icon::Copy` in the dim ink (the text ink on hover), tooltip `Copy`. Click: `cx.write_to_clipboard(ClipboardItem::new_string(code_copy_text(code)))`.
  - Then ✓ (`Icon::Check`) for 1.5 s. Keep "copied" as an `Rc<Cell<Option<(usize, usize, Instant)>>>` on `Draw` (the block's row and its index in the row). Schedule one redraw at the deadline, not a frame loop.
  - The reader's header and its Copy stay as they are.
- [ ] **Step 3: Scene:** a chat row with the two-line `docker compose` block; its copy button's bounds sit inside the block's top-right. Run → PASS; the gate.
- [ ] **Step 4: Commit**: `Chore (Task 8): code blocks in the chat get a copy icon`.

### Task 9: Folder picker A

**Files:**
- Modify: `src/folder_picker.rs` (`PickerRow::Folder` gains `name`, `place`, `shortcut`; pure helpers)
- Modify: `src/agent_box_menus.rs:369` (`picker_row` draws the two-line row; actions with icons; footer; ↑↓ highlight; ⌘1–⌘9)
- Modify: `src/main.rs` (the picker's key bindings, in the picker search's own key context)

**Interfaces:**
- Produces: `PickerRow::Folder { path: PathBuf, shown: String, name: String, place: String, current: bool, shortcut: Option<u8> }`. `pub fn folder_words(path: &Path, home: Option<&Path>) -> (String, String)`. `pub fn moved(rows: &[PickerRow], at: Option<usize>, down: bool) -> Option<usize>` (the next selectable row: a Folder or an action, never a Heading). The open picker keeps `selected: Option<usize>`.

- [ ] **Step 1: Failing tests** in `folder_picker.rs`:
  - `a_folder_reads_name_then_place`: `folder_words("/Users/me/code/fuxx-cli", Some("/Users/me")) == ("fuxx-cli", "~/code")`; home itself → `("Home", "~")`; `/` → `("/", "")`; outside home `/opt/x` → `("x", "/opt")`.
  - `recent_rows_get_cmd_digits`: with 11 recent folders and an empty query, the Recent rows' `shortcut`s are `Some(1)..=Some(9)` then `None`, `None`; This Computer rows (while searching) get `None`.
  - `arrows_skip_headings`: `moved` from `None` going down lands on the first Folder; past the last row it stays on the last; going up from the first selectable row stays there.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.** `folder_row` fills `name` / `place` / `shortcut`; fuzzy matching still uses `shown`. The row (mockup A):
  - A 26 pt tile with a folder glyph (`Icon::Folder` if it exists, else add the Lucide `folder` and `house` SVGs to `icons.rs`'s FILES). The current row's tile is in the accent's soft fill.
  - `name` semibold, with `place` dim under it.
  - On the right: ✓ when current, else the dim `⌘N` key cap.
  
  Then a divider, and the actions with leading icons: This Computer ›, New Project…, No Project. Footer: `↑↓ move · ↵ open · esc close`, in dim text at the floor size.
  
  ↑/↓ in the search move `selected` (shown with the hover fill); ↵ picks `selected`, else the first folder (as today). ⌘1–⌘9 pick that Recent row. Bind them with the picker search's key context; check the existing ones in `main.rs` first and reuse that context.
- [ ] **Step 4: Run** → PASS; the gate.
- [ ] **Step 5: Commit** `src/folder_picker.rs src/agent_box_menus.rs src/main.rs` (+ `src/icons.rs` and SVGs if added): `Chore (Task 9): the folder picker leads with each folder's name, its place under it; ⌘1–⌘9, ↑↓, icons and a key-hint footer`.

### Task 10: Changed-files popover A, without the duplicate paths

**Files:**
- Create: `src/changed_files.rs`: the facts git and the disk give, and the grouping. Pure, except `read_facts`.
- Modify: `src/main.rs` (`mod changed_files;`)
- Modify: `src/agent_tab.rs` (keep `ledger_facts`, read off the UI thread when the ledger's paths change; `ledger_folded: HashSet<String>`)
- Modify: `src/agent_box_menus.rs:996-1075` (`ledger_menu` draws the groups)

**Interfaces:**
- Produces:
  - `pub enum FileStatus { Added, Modified, Deleted }`
  - `pub struct FileFacts { pub status: FileStatus, pub plus: Option<u32>, pub minus: Option<u32>, pub is_file: bool }`
  - `pub fn read_facts(root: &Path, paths: &[PathBuf]) -> HashMap<PathBuf, FileFacts>`: runs `git status --porcelain=v1 -z -- <paths>` and `git diff --numstat HEAD -- <paths>`. An untracked file's plus is its line count. With no git or no `HEAD`, status comes from existence (missing → Deleted) and counts are None.
  - `pub struct FileRow { pub path: PathBuf, pub name: String, pub status: FileStatus, pub plus: Option<u32>, pub minus: Option<u32> }`
  - `pub struct Group { pub folder: String, pub rows: Vec<FileRow> }` (folder relative to the tab's cwd; `""` for files at its top)
  - `pub fn changed_groups(changed: &[Changed], facts: &HashMap<PathBuf, FileFacts>, cwd: &Path) -> Vec<Group>` (the ledger's counts win; facts fill the gaps)
  - `pub fn kept_paths(paths: &[PathBuf], facts: &HashMap<PathBuf, FileFacts>) -> Vec<PathBuf>` (scripted / read: only those facts call files, or Deleted)
  - `pub fn total(groups: &[Group]) -> (usize, u32, u32)`

- [ ] **Step 1: Failing tests** in `changed_files.rs`. Use `test_temp::dir` and a real `git init` repo for the I/O ones:
  - `groups_sort_by_folder_and_name`: `src/b.rs`, `assets/x.svg`, `src/a.rs`, `README.md` → groups `""` [README.md], `assets` [x.svg], `src` [a.rs, b.rs].
  - `ledger_counts_win_and_facts_fill_gaps`: Changed `src/a.rs` (+3 −1) with facts (+9 −9) → +3 −1; Changed `src/b.rs` (None, None) with facts (+4 −2, Modified) → +4 −2.
  - `script_named_folders_and_missing_files_are_dropped`: facts say `icons/lucide` `is_file: false` and `icons/shapes/x.svg` has no fact (missing, not tracked) → `kept_paths` drops both and keeps `assets/icons/shapes/x.svg` (`is_file: true`).
  - `facts_in_a_repo`: a committed file edited (+2 −1) → Modified, `Some(2)`, `Some(1)`; a new untracked 3-line file → Added, `Some(3)`, `Some(0)`; a deleted tracked file → Deleted.
  - `facts_without_git`: a plain folder → existing file Modified with None counts; a missing one → Deleted.
  - `facts_without_head`: `git init` with nothing committed → new files are Added, no panic.
  - `total_sums_rows`: two rows (+3 −1) and (+4 None) → `(2, 7, 1)`.
- [ ] **Step 2: Run** `cargo test --bin fuxx changed_files` → FAIL.
- [ ] **Step 3: Implement** `changed_files.rs` (find git with `git_branch::find_git`, as `commit_line` does).
  
  In `agent_tab.rs`, when the ledger's changed / scripted / read path set differs from the last asked set, spawn `read_facts` on the background executor (as `commit_line::check_in_progress` does) and store the result. Don't ask again for the same set.
  
  `ledger_menu` (mockup A):
  - A total line: **N files changed** with +P in the green ink and −M in the red one.
  - Per group: a `▾ folder  count` header row that folds on click (keyed in `ledger_folded`). Groups with `""` have no header.
  - A file row: an A/M/D badge (16 pt, its tint), the file name, +/− on the right.
  - Click: `OpenDiff(path)`. ⌥-click: `OpenFile(path)` (check `event.modifiers().alt` on the click).
  - "Touched by scripts" and "Read" use `kept_paths`, are grouped the same way, and have no counts. Until facts arrive they show nothing, so no junk flashes.
  - Checks stay as they are.
- [ ] **Step 4: Run** → PASS; the gate.
- [ ] **Step 5: Commit** `src/changed_files.rs src/main.rs src/agent_tab.rs src/agent_box_menus.rs`: `Chore (Task 10): the changed-files popover groups files by folder with A/M/D and +/− from git; paths named in script text no longer show as files`.

### Task 11: The Plan card (Claude's Plan Mode)

**Files:**
- Modify: `src/providers/mod.rs` (`PlanDraft`; `Event::Permission` gains `plan`)
- Modify: `src/providers/claude.rs:1966` (`permission()` reads `ExitPlanMode`)
- Modify: `src/agent_conversation.rs:82,446` (`Item::Permission` gains `plan`)
- Create: `src/agent_plan_card.rs` (the card: header, folded markdown body, footer, answered state)
- Modify: `src/agent_rows.rs:442` (a Permission with a plan draws the plan card)
- Modify: `src/agent_tab.rs` (`blocks_of` parses a plan's markdown; the answers; `plan_note` for Keep planning; the box's placeholder)
- Modify: every `Event::Permission { … }` / `Item::Permission { … }` literal (`plan: None`; the list is at `grep -rn 'Event::Permission {' src`)

**Interfaces:**
- Produces:
  - `pub struct PlanDraft { pub title: String, pub markdown: String, pub file: Option<PathBuf> }` (Clone, Debug, PartialEq)
  - `Event::Permission { …, plan: Option<PlanDraft> }`; `Item::Permission { …, plan: Option<Box<PlanDraft>> }`
  - `pub fn plan_title(markdown: &str) -> String` in `providers/mod.rs` (generic): the first `# ` heading with a leading `Plan:` taken off, else `"Plan"`.
  - The answers: `Answer::AlwaysAllowed` = Approve · auto-accept edits; `Answer::Allowed` = Approve · ask per edit; `Answer::Denied(Some(note))` = Kept planning.

- [ ] **Step 1: Context7.** Check the Claude Agent SDK's `can_use_tool` allow response, its `updatedPermissions` with `{"type":"setMode","mode":"acceptEdits","destination":"session"}`, and confirm the shape. Record the answer in the commit message.
- [ ] **Step 2: Failing tests:**
  - claude.rs `exit_plan_mode_is_a_plan`: decode `claude-set-mode.jsonl`'s line 159. It's one `Event::Permission` whose `plan == Some(PlanDraft { title: "Create hello.txt", markdown: <the input's plan>, file: Some(~ expanded "~/.claude/plans/create-a-file-named-silly-catmull.md") })`.
  - claude.rs `approving_a_plan_with_auto_accept_sets_the_mode`: after that decode, `encode(&Action::AlwaysAllow { id })`'s response has `behavior: "allow"` and `updatedPermissions == [{"type":"setMode","mode":"acceptEdits","destination":"session"}]`; `Action::Allow` has no `updatedPermissions`; `Action::Deny { reason: Some("split step 2") }` denies with a message containing `split step 2`.
  - mod.rs `plan_title_falls_back`: `plan_title("# Plan: Retry\n…") == "Retry"`; `plan_title("# Retry") == "Retry"`; `plan_title("no heading") == "Plan"`; `plan_title("") == "Plan"`.
  - agent_conversation.rs `a_plan_expires_with_the_turn`: a Permission with a plan, then the turn's end → `answer == Some(Answer::Expired)` and `plan` is still there.
  - agent_tab.rs `answer_line` cases: a plan answered `AlwaysAllowed` reads `Approved · auto-accept edits ✓`, `Allowed` reads `Approved ✓`, `Denied(Some(n))` reads `Kept planning: n`.
- [ ] **Step 3: Run** → FAIL.
- [ ] **Step 4: Implement.**
  - **claude.rs:** when `name == "ExitPlanMode"` and `input.plan` is a string, build the `PlanDraft` (expand a leading `~/` with the home dir). Store the pending entry with `suggestions = [{"type":"setMode","mode":"acceptEdits","destination":"session"}]`, so `AlwaysAllow` sends it.
  - **Conversation:** carry `plan` through.
  - **`blocks_of`:** also parses a plan's markdown (cached like agent text).
  - **The card (`agent_plan_card.rs`, the spec's refined chore 5, which wins over the first mockup):**
    - An accent rim (not red). Header: ☰ **Claude's plan** in the accent and the file chip with ↗ (click: `OpenFile`). Then the title, semibold.
    - Body: `md_preview::column`, clipped to about 12 lines with a fade and **Show the whole plan ▾**. Open, it's the whole plan in place (no inner scroll) with **Show less ▴** at its top and bottom. The state is kept per item id on the tab, like `toggle_summary`. Closing it scrolls the card's top into view.
    - Outline chips under the title when the plan has 3 or more `##` headings (pure `pub fn plan_outline(blocks: &[Md]) -> Vec<String>`; test `the_outline_lists_level_two_headings`). A click scrolls the chat to that heading's block.
    - Footer, while open: **Approve · auto-accept edits ↵** (accent pill) → `answer(AlwaysAllowed)`; **Approve · ask per edit** → `answer(Allowed)`; **Keep planning…** (quiet) → `plan_note = true`, focus the box. Under them: `Or type in the box below to tell Claude what to change in the plan.`
    - While a plan waits, the box's placeholder is `Tell Claude what to change in the plan… (optional)` and the status line reads `Waiting for you · Claude's plan is ready`.
    - `submit` on a waiting plan: empty text and `plan_note` off → `AlwaysAllowed`; empty and `plan_note` on → nothing; text → `Denied(Some(text))`. Test `submit_on_a_plan` covers the three. Typing any text turns `plan_note` on, which hides the footer's buttons; Esc turns it off.
    - **The pinned answer bar:** while a waiting plan's footer is out of the chat's view, a slim bar above the agent box shows ☰, the title (cut with "…") and the three answers. Use the footer's painted bounds against the list's viewport, as `ask_dock` does for the question card (`agent_ask_view.rs:612`). Reuse that dock's placement.
    - Answered or expired: one line, ☰, the title, `N steps · ▸ show` and the answer line (green when approved). ▸ opens the plan for reading.
  - **The tab's mode chip** follows Claude's own mode change (R9 already does that).
- [ ] **Step 5: Run** → PASS; the gate.
- [ ] **Step 6: Commit** the files above: `Chore (Task 11): Claude's Plan Mode plan is a readable Plan card — approve with auto-accept or per-edit, or keep planning with a note`.

### Task 12: No ◐ before work has started

**Files:** Modify `src/plan_progress.rs:118-135` (`match_commits`) and its tests.

- [ ] **Step 1: Failing tests:**
  - `nothing_started_shows_nothing_building`: three Next tasks and no commits naming any → all three stay `Next`.
  - `after_a_done_task_the_next_is_building`: Task 1 named by a commit → Task 1 `Done`, Task 2 `Building`, Task 3 `Next`.
  - `a_ticked_step_is_building`: `from_checklist` on a plan whose Task 1 has 1 of 3 boxes ticked → Task 1 `Building`. This already holds; it pins it.
  
  Run → the first FAILS.
- [ ] **Step 2: Implement:** the fallback (the first `Next` becomes `Building`) applies only when at least one task is already `Done`. Update any existing test that expected ◐ on an untouched plan, and say which in the commit.
- [ ] **Step 3: Run** → PASS; the gate.
- [ ] **Step 4: Commit**: `Chore (Task 12): the plan tab shows no task in progress before any work has started`.

### Task 13: The branch chip keeps up with the repo

**Files:** Modify `src/agent_tab.rs` (`read_branch` at ~4890, its callers), and the window-activation hook the workspace already has (search `observe_window_activation`).

**Interfaces:** Produces `pub fn rereads_branch(event: &Event) -> bool` in `agent_tab.rs` (pure): true for a shell tool call's finish and for the turn's end.

- [ ] **Step 1: Failing test** `the_branch_is_read_again_after_shell_commands_and_turns`: a finished Bash tool → true; a finished Read → false; the turn's end → true; a text delta → false. Run → FAIL.
- [ ] **Step 2: Implement:** when `rereads_branch` holds, call `read_branch`, debounced so it runs at most once per 500 ms (a `last_branch_read: Option<Instant>`; a call inside the window schedules one at the window's end). Also call `read_branch` for every agent tab when fuxx's window is activated. `read_branch` already drops answers for a folder the agent has left, and redraws only when the branch differs.
- [ ] **Step 3: Run** → PASS; the gate.
- [ ] **Step 4: Commit**: `Chore (Task 13): the agent box's branch chip follows a switch made by the agent or outside fuxx`.

### Task 14: Docs, screenshots, the review

- [ ] **Step 1:** Note 93, "Chores after M11ar": one entry per chore (your words, what was built, the commits), and a "Checklist for you" with the GUI-only items:
  - the commit box's 2 rows and grey buttons
  - the clock panel centred
  - the hover icons staying
  - the plan tab's flush corner
  - a click elsewhere clearing a track column's border
  - the plan popover left-aligned
  - your bubble wrapping in a narrow pane
  - the copy icon on a chat code block, and its ✓
  - the plan tab showing no ◐ until a task is under way
  - the branch chip following a `git switch` by the agent and in another terminal
  - the picker's look, ↑↓ and ⌘N
  - the popover's groups, ⌥-click and the duplicates gone
  - a real Plan Mode run in each of the three answers
  
  Add a Progress Log entry at the top and update the status snapshot.
- [ ] **Step 2:** `scripts/ui-shots.sh --changed master`; update the scenes these chores changed (at least `agent-box-folder-picker`, `agent-ledger-popover`, `agent-tab-permission`, the commit line's and the clock's), and add a Plan card scene built from the `claude-set-mode.jsonl` events. Expect the same display-dependent failures as last run, and say which.
- [ ] **Step 3:** One whole-branch review on the most capable model (superpowers:requesting-code-review); one fix wave; re-run the gate and `--changed` scenes.
- [ ] **Step 4:** Commit the docs by name; `prune-target` per the build-folder memory. Merge and push wait for your word.
