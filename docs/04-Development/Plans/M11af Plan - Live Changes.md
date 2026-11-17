---
tags: [development, plan, milestone]
milestone: M11af
status: built
updated: 2026-11-17
---

# M11af Plan — Live changes, with who wrote them

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[82 - Milestone 11af - Live Changes]] |
| **Status** | 🟨 Built 2026-11-17, merged to master (`9afbb29`), not pushed; waiting for your on-screen check |
| **Tasks done** | 8 / 8 |
| **Baselines** | 1,787 tests (1 ignored); clippy 14 (bin) / 27 (test) default, 16 / 29 with `ui-shots`; fmt clean |
| **▶ Pick up here** | Your on-screen check in note 82, then the merge |
| **Decisions used** | Yours: approach A, "All it recognises" (every sidebar-recognised agent gets a mark; agy joins `procs::AGENTS`), "approve". The numbers in note 82 are the AI's calls, approved with the design. |
| **Needs your eyes** | 👀 the on-screen check in note 82, after Task 8 |

**Goal:** A file tab shows which lines just changed on disk (a glow that cools over 60 s), who changed each line this
session (the agent's letter, exact or guessed, or yours), and can follow the agent's newest edit.

**Architecture:** Two pure modules carry the logic, unit-tested in `cargo test`: `src/live_lines.rs` (compare two
texts, keep a per-line record that follows its lines, glow levels and age words) and `src/edit_ledger.rs` (an
app-wide record of agents' file edits and working spans, and the rule that decides who made a change). Agent tabs and
terminals feed the ledger through the workspace; the file tab compares on each reload (off the UI thread, with the
read), asks the ledger, and draws. The glow is stepped and recomputed on the file tab's existing 1 s poll (M11p: no new
always-on timer).

**Tech Stack:** Rust 2024, GPUI (pinned rev, unchanged), **`similar = 3.2.1`** (new; checked 2026-11-17 with Context7
and crates.io: `similar::capture_diff_slices_deadline(Algorithm::Myers, &old, &new, Some(deadline)) -> Vec<DiffOp>`,
`DiffOp::as_tag_tuple() -> (DiffTag, Range<usize>, Range<usize>)`; with a deadline it returns an approximate diff
rather than failing, so "gave up" is measured by elapsed time). `default-features = false, features = ["std"]`.

**Spec:** [[82 - Milestone 11af - Live Changes]]

## Global Constraints

- Branch `m11af-live-changes` from master `e9f3729`, in the main checkout (no worktree). Commit per task; merge to
  master after the whole-branch review (standing word); pushing is the user's call, after the private-name grep.
- **Gate per task:** `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features
  ui-shots && cargo fmt --check`, then the task's scenes with `scripts/ui-shots.sh --changed master`, and the full
  `scripts/ui-shots.sh` once before the commit of a task that touches drawing. Clippy baselines: 14 / 27 (default) and
  16 / 29 (`ui-shots`) — Task 1 re-records them; no new warnings.
- **M11p:** nothing visible changes synchronously inside any `render`; no new always-on timer. The only new timer is
  the one-shot 2 s revision check (Task 5), which ends itself.
- **Provider-specific code stays in `src/providers/<agent>.rs`**; generic code never matches tool names.
- **No work-project names** in code, tests, fixtures or docs (example folders `~/code/my-app`, `~/code/scratch`).
- Numbers and words exactly as note 82: glow 60 s in three steps — level 3 under 15 s, level 2 under 35 s, level 1
  under 60 s; tint alphas start at .20 / .11 / .05 of the accent; age words `just now` (under 5 s) and `N s ago`;
  compare limit **50 ms**; ledger kept **10 min**; slack **2 s**; revision window **2 s** after the read; Follow
  places the change `DIFF_TOP` rows from the top (as a diff opens). Tooltips `claude · 12 s ago`,
  `probably claude · 12 s ago (it was working)`, `you · 3 min ago`. Pill `Follow claude` / `● Following claude`.
  `·` is U+00B7 with spaces.
- Rendering is checked in ui-shots scenes (`--features ui-shots`); logic in `cargo test`. Ages in scenes come from
  explicit times (frozen tabs never poll).

## Review Focus

1. **The agent rewrites the whole file with one line different** (Claude's Write): only that line is marked. Pinned in
   Task 1 (`whole_rewrite_marks_only_the_changed_line`).
2. **A file gains or loses its final newline, or uses CRLF:** no line is marked for the terminator alone. Pinned in
   Task 1 (`terminators_alone_mark_nothing`).
3. **The file is deleted and written again** (some tools replace by delete + create): the new text is compared with
   the last text seen, not marked all new. Pinned in Task 5 (`missing_then_back_compares_with_last_text`).
4. **Two saves within one poll, by two writers:** one read sees both; the read is decided once (an exact agent edit
   wins) and nothing breaks on lines that moved. Pinned in Task 3 (`two_writers_in_one_read_agent_wins`).
5. **Switching to Diff or Preview and back:** marks and glow are kept, Follow is off. Pinned in Task 7
   (`mode_switch_keeps_marks_turns_follow_off`).

---

### Task 1: Compare texts and keep a line record (`src/live_lines.rs`)

**Files:**
- Modify: `Cargo.toml` (add `similar = { version = "3.2.1", default-features = false, features = ["std"] }`)
- Create: `src/live_lines.rs` (declared in `src/main.rs` with the other modules)

**Interfaces:**
- Produces:
  ```rust
  pub const LIMIT: Duration = Duration::from_millis(50);
  pub const WARM: Duration = Duration::from_secs(60);
  /// Which new lines are new or changed (blocks), and where each old line went (None: removed or changed).
  pub struct Compared { pub blocks: Vec<Range<usize>>, pub new_of_old: Vec<Option<usize>> }
  pub fn compare(old: &[&str], new: &[&str], limit: Duration) -> Option<Compared>; // None: gave up
  #[derive(Clone, Debug, PartialEq)] pub enum Who { Agent(AgentMark), You }
  #[derive(Clone, Debug, PartialEq)] pub struct AgentMark { pub name: Arc<str>, pub letter: char, pub rgb: Option<u32> }
  #[derive(Clone, Copy, Debug, PartialEq)] pub enum Sure { Exact, Guess }
  #[derive(Clone, Debug, PartialEq)] pub struct Mark { pub who: Who, pub sure: Sure, pub when: SystemTime }
  #[derive(Default)] pub struct LineRecord { /* one Option<Mark> per line, plus the last apply's blocks */ }
  impl LineRecord {
      pub fn baseline(lines: usize) -> LineRecord;
      pub fn apply(&mut self, c: &Compared, new_len: usize, mark: Mark); // carries marks, stamps the blocks
      pub fn revise_last(&mut self, who: Who, sure: Sure);               // the last apply's blocks only
      pub fn clear(&mut self, lines: usize);
      pub fn mark(&self, line: usize) -> Option<&Mark>;
      pub fn any(&self) -> bool;
      pub fn last_blocks(&self) -> &[Range<usize>];
      pub fn warm_blocks(&self, now: SystemTime) -> Vec<(Range<usize>, u8)>; // runs of lines at one level
  }
  pub fn glow_level(age: Duration) -> Option<u8>; // 3, 2, 1, None
  pub fn age_words(age: Duration) -> String;      // "just now", "12 s ago"
  pub fn letter_of(name: &str) -> char;           // first alphanumeric, uppercased; '?' if none
  ```

- [ ] **Step 1: Record the baselines.** Run the gate on the clean branch; write the test count and both clippy counts
  into this plan's "At a glance".
- [ ] **Step 2: Write the failing tests** in `src/live_lines.rs`:
  - `compare_marks_inserted_and_changed_lines`: old `["a","b","c"]`, new `["a","B","c","d"]` → `blocks == [1..2, 3..4]`,
    `new_of_old == [Some(0), None, Some(2)]`.
  - `whole_rewrite_marks_only_the_changed_line`: 200 equal lines, line 120 changed → `blocks == [120..121]`.
  - `terminators_alone_mark_nothing`: lines are compared without their terminators (the file tab passes its
    `doc.lines` slices with `\r` and `\n` trimmed); a CRLF text and its LF twin, and a text with and without a final
    newline, give equal slices and `compare` finds no blocks.
  - `gave_up_over_the_limit`: `compare(.., Duration::ZERO)` on two different 5,000-line texts → `None`.
  - `marks_follow_their_lines`: baseline 3 lines; mark line 1; insert two lines at the top → the old mark is on line 3,
    lines 0–1 carry the new mark.
  - `revise_last_changes_only_the_last_blocks`; `clear_drops_every_mark`.
  - `glow_steps`: 0 s → 3, 14.9 s → 3, 15 s → 2, 34.9 s → 2, 35 s → 1, 59.9 s → 1, 60 s → None.
  - `age_words_say_just_now_then_seconds`: 0 → `just now`, 4.9 s → `just now`, 5 s → `5 s ago`, 59 s → `59 s ago`.
  - `warm_blocks_group_by_level`: two adjacent lines of different ages give two runs.
  - `letter_of_names`: `claude` → `C`, `agy` → `A`, `Fitz Gerald` → `F`, `""` → `?`.
- [ ] **Step 3: Run** `cargo test --bin fuxx live_lines` — FAIL.
- [ ] **Step 4: Implement.** `compare` uses `capture_diff_slices_deadline(Algorithm::Myers, …, Some(start + limit))`
  and returns `None` when the elapsed time reached `limit`; `Insert` and `Replace` new ranges are blocks (adjacent
  ones merged); `Equal` fills `new_of_old`.
- [ ] **Step 5: Run** the tests — PASS; the gate.
- [ ] **Step 6: Commit** `M11af Task 1: live_lines — compare texts, line record, glow steps`.

### Task 2: Which tool calls write which files; agy joins the known agents

**Files:**
- Modify: `src/providers/mod.rs` (`Event::ToolStarted` gains `writes: Vec<String>`), `src/providers/claude.rs`,
  `src/providers/agy.rs`, every `Event::ToolStarted { .. }` construction and pattern (compiler-led)
- Modify: `src/procs.rs:61` (`AGENTS` gains `"agy"`)

**Interfaces:**
- Produces: `Event::ToolStarted { id, name, summary, parent, detail, writes: Vec<String> }` — the paths the call
  writes, as the provider read them (absolute or relative to the agent's cwd); empty for a call that writes no file.

- [ ] **Step 1: Write the failing tests:**
  - `claude.rs` `edit_tools_name_the_files_they_write`: the recorded fixtures' `Edit` and `Write` calls give
    `writes == [file_path]`; a `MultiEdit` (`file_path`) and a `NotebookEdit` (`notebook_path`) built inline give their
    paths; `Bash` and `Read` give `[]`.
  - `agy.rs` `agy_tool_calls_name_written_files_when_they_say`: from agy's recorded fixtures, a file-writing tool whose
    event carries a path gives it. If no agy event carries a path, the test asserts `writes` is empty for every agy
    tool call, and note 82's "Learned" records that agy's edits are always guessed.
  - `procs.rs` `agy_is_a_known_agent`: `is_known_agent("agy")`, `program_name("agy --foo") == "agy"`.
- [ ] **Step 2: Run** them — FAIL.
- [ ] **Step 3: Implement** in the provider files only.
- [ ] **Step 4: Run** — PASS; the gate (sidebar scenes still pass; a hand-typed agy row now reads `agy`).
- [ ] **Step 5: Commit** `M11af Task 2: providers say which files a tool call writes; agy is a known agent`.

### Task 3: The edit ledger and who decides (`src/edit_ledger.rs`)

**Files:**
- Create: `src/edit_ledger.rs` (declared in `src/main.rs`)

**Interfaces:**
- Consumes: `live_lines::{AgentMark, Who, Sure}`, `layout_model::WorkspaceId`.
- Produces:
  ```rust
  pub const KEEP: Duration = Duration::from_secs(600);
  pub const SLACK: Duration = Duration::from_secs(2);
  /// An app-wide GPUI global (`impl Global`), set once in `main`.
  #[derive(Default)] pub struct Ledger { /* edits, working spans */ }
  impl Ledger {
      /// `paths` canonical (the caller joins relative ones to the agent's cwd and canonicalizes).
      pub fn edit_started(&mut self, call: &str, agent: AgentMark, paths: Vec<PathBuf>, at: Instant);
      pub fn edit_finished(&mut self, call: &str, ok: bool, at: Instant);
      /// `key`: a terminal's session id or an agent tab's slot id; `on`: Working began or ended.
      pub fn working(&mut self, ws: WorkspaceId, agent: AgentMark, key: u64, on: bool, at: Instant);
      /// Who changed `path` between `since` (the tab's previous read) and `now`; the bool: an edit to `path`
      /// is still open (started, not finished).
      pub fn decide(&self, path: &Path, ws: Option<WorkspaceId>, since: Instant, now: Instant) -> (Who, Sure, bool);
      /// An agent edit to `path` that finished ok in `read_at ..= read_at + SLACK`.
      pub fn revise(&self, path: &Path, read_at: Instant) -> Option<AgentMark>;
      pub fn prune(&mut self, now: Instant);
  }
  pub fn agent_mark_for(name: &str) -> AgentMark; // see Task 4
  ```

- [ ] **Step 1: Write the failing tests** (times from one `Instant` plus offsets):
  - `an_agent_tab_edit_since_the_last_read_is_exact` (also when `ws` differs from the agent's workspace).
  - `an_edit_finished_before_since_minus_slack_does_not_count`; `one_within_the_slack_counts`.
  - `an_open_edit_counts_as_exact_and_is_still_open`; `a_failed_edit_does_not_count`.
  - `a_working_agent_in_the_same_workspace_is_a_guess`; `a_working_agent_in_another_workspace_is_not`.
  - `two_agents_working_the_last_started_wins`.
  - `nothing_else_is_yours` (→ `(Who::You, Sure::Exact, false)`).
  - `two_writers_in_one_read_agent_wins` (an exact edit and your save in one read → the agent, exact).
  - `revise_finds_a_late_finish`; `revise_ignores_one_after_the_window`.
  - `prune_drops_what_is_older_than_ten_minutes` (a span still on is kept).
- [ ] **Step 2: Run** `cargo test --bin fuxx edit_ledger` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS; the gate.
- [ ] **Step 5: Commit** `M11af Task 3: the edit ledger and the who-decides rule`.

### Task 4: Feed the ledger from agent tabs and terminals

**Files:**
- Modify: `src/agent_tab.rs:1240` (the `Event::ToolStarted` arm) and the `ToolFinished` handling; `AgentTabEvent`
  gains `Edit { call: String, paths: Vec<PathBuf> }` (joined to the tab's `start.cwd`) and
  `EditDone { call: String, ok: bool }`
- Modify: `src/workspace_agent.rs` (handle the two events next to `AgentTabEvent::Status` at :772;
  `set_agent_status` reports `Ledger::working` on/off)
- Modify: `src/workspace.rs:2231` `set_status`: a terminal whose program is a known agent (`procs::is_known_agent`
  on `ProgramState::Running(name)`) or whose preset is an agent preset (`presets::agent_for`, as `note_agents` at
  :1325) reports `working` on/off — never a plain shell running a build
- Modify: `src/main.rs` (`cx.set_global(Ledger::default())`)

**Interfaces:**
- Consumes: Task 2's `writes`, Task 3's `Ledger`.
- Produces: `edit_ledger::agent_mark_for(name)` — a provider's name (`providers::by_name`), `letter_of(name)`,
  `Some(info().badge_rgb)`; any other agent: its program name, `letter_of`, `None` (drawn grey).

- [ ] **Step 1: Write the failing tests:** `agent_tab` `an_edit_tool_call_emits_edit_with_paths_joined_to_cwd` (a decoded
  `Write` with relative path `hello.txt`, cwd `/tmp/fx` → `Edit { paths: ["/tmp/fx/hello.txt"] }`, then `EditDone { ok:
  true }` on its finish); `edit_ledger` `agent_mark_for_known_and_unknown_agents` (`claude` → `C` with Claude's badge
  colour; `codex` → `C`, `rgb: None`).
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** The workspace calls `Ledger::prune(now)` whenever it writes to it.
- [ ] **Step 4: Run** — PASS; the gate (agent-tab scenes still pass).
- [ ] **Step 5: Commit** `M11af Task 4: agent tabs and agent terminals feed the ledger`.

### Task 5: The file tab keeps a record on each reload

**Files:**
- Modify: `src/file_tab.rs` — `FileTab` gains `live: Live` (the `LineRecord`, the last text's trimmed line slices'
  source `Doc`, the last read's `Instant`, `workspace: Option<WorkspaceId>`, `your_letter: Option<char>`,
  `follow: bool`, `revising: bool`); `read_file`'s `Loaded::Text` carries `Option<Compared>` made on the background
  executor against the previous text; `loaded` asks `Ledger::decide` and applies; `Loaded::Missing` keeps the record
  and the last text; `PlainTooLarge`, `Refuse` or a `None` compare clear the record
- Modify: `src/workspace.rs` — wherever file tabs get `set_shown`, also `set_workspace(ws)`

**Interfaces:**
- Consumes: Tasks 1, 3, 4.
- Produces: `FileTab::set_workspace(&mut self, ws: Option<WorkspaceId>)`; `FileTab::live_marks(&self) -> &LineRecord`;
  under `ui-shots`, `FileTab::set_live_for_shot(&mut self, record: LineRecord, now: SystemTime, follow: Option<bool>)`;
  `fn your_letter(git_name: Option<&str>, account_name: Option<&str>) -> char` (pure).

- [ ] **Step 1: Write the failing tests** (`file_tab::tests`, pure helpers split out where `FileTab` needs GPUI):
  `first_read_is_a_baseline`; `missing_then_back_compares_with_last_text`; `plain_too_large_or_gave_up_clears_marks`;
  `when_is_the_mtime_if_earlier` (a change seen 10 min after its mtime is marked and not warm);
  `your_letter_from_git_name_then_account_then_y`.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** After a read whose decision was still open, or `Guess`, or
  `You`, one 2 s timer (`revising` keeps it to one at a time) calls `Ledger::revise` and `LineRecord::revise_last`,
  notifies if changed, and ends. Your letter: `git config user.name` with the git program and repo already found
  (`git_place`), else `id -F`, run once per tab off the UI thread.
- [ ] **Step 4: Run** — PASS; the gate.
- [ ] **Step 5: Commit** `M11af Task 5: file tabs keep who-changed-what on each reload`.

### Task 6: Draw the glow, the age words and the marks

**Files:**
- Modify: `src/file_tab.rs` — `paint_body` (tints under the text, before selection and find; File mode only); the
  `Gutter::Plain` geometry at :1197 gains a **16 pt mark column only while `live.any()`**, so a tab with no marks looks
  exactly as today; age words at the end of each warm run's first line; a hover tooltip on a mark (as the bar's
  tooltips); the 1 s poll calls `refresh_live(now)` and notifies only when a run's level or words changed
- Modify: `src/ui_shots/file_tab.rs` — new scenes in its `SCENES`

- [ ] **Step 1: Write the failing scenes** (dark and light): `file-live-glow` (runs at 2 s, 20 s, 40 s and 70 s: three
  tints, one untinted; `just now` / `20 s ago` / `40 s ago` once per run); `file-live-marks` (exact Claude filled,
  guessed Claude hollow, codex grey hollow, yours teal filled); `file-live-none` (no marks: geometry and pixels equal
  a plain file tab's); `file-live-tooltip` (hover a guessed mark → `probably claude · 12 s ago (it was working)`);
  `file-live-modes` (Diff and Preview show no tint).
- [ ] **Step 2: Run** `scripts/ui-shots.sh --changed master` — the new scenes FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS; the full `scripts/ui-shots.sh`; existing scenes' fingerprints unchanged (distance 0).
- [ ] **Step 5: Commit** `M11af Task 6: draw the glow, age words and who marks`.

### Task 7: Follow

**Files:**
- Modify: `src/file_tab.rs` — the pill in the bar's controls (File mode, once an agent mark exists; named from the
  newest agent mark); `set_scroll` (:3329) and mode switches turn `follow` off; after a reload with agent blocks and
  `follow` on, `follow_to(line)` scrolls without passing through `set_scroll`'s off-switch, so the first block's first
  line sits `DIFF_TOP` rows from the top unless already in view
- Modify: `src/ui_shots/file_tab.rs`

**Interfaces:**
- Produces: `fn follow_scroll(view: Range<usize>, line: usize, top_rows: usize) -> Option<usize>` (pure: the first row
  to show, None when `line` is in view).

- [ ] **Step 1: Write the failing tests and scenes:** `follow_scroll_is_none_when_in_view`, `follow_scroll_puts_line_at_top_rows`,
  `follow_off_by_default`; scenes `file-follow-pill` (off / on), `file-follow-scrolls` (a change at line 400 comes to
  `DIFF_TOP` rows from the top), `file-follow-wheel-stops`, `mode_switch_keeps_marks_turns_follow_off`.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the full `scripts/ui-shots.sh`.
- [ ] **Step 5: Commit** `M11af Task 7: Follow keeps the agent's newest edit in view`.

### Task 8: Docs, verification, hand-off

**Files:**
- Modify: note 82 ("What was built", "On-screen check", status), `docs/04-Development/40 - Roadmap.md`,
  `docs/06-Progress-Log/60 - Progress Log.md`, this plan's table, `src/file_tab.rs` module docs (a "Live changes"
  paragraph), and the repo's module list (`README` / `STRUCTURE`) if it lists modules

- [ ] **Step 1: Idle CPU (M11p anti-goal):** a file tab shown, nothing warm: `top -l 6 -s 5 -pid <fuxx>` median on
  the branch vs master, both in note 82; they match within noise.
- [ ] **Step 2: Write note 82's on-screen check** (about 10 items): an agent-tab Claude edits a file — glow, filled
  `C`; `claude` in a plain terminal — hollow `C`; save from your editor — `F`; codex if installed — grey; Follow on,
  the wheel stops it; Diff and Preview untouched; a file over the limit — no marks; a hand-typed `agy` row reads `agy`.
- [ ] **Step 3: Full gate** and full `scripts/ui-shots.sh`; numbers into note 82 and the Progress Log.
- [ ] **Step 4: Commit** `M11af Task 8: docs and hand-off`.
