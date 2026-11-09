---
tags: [development, plan, milestone]
milestone: M11l
status: planned
updated: 2026-11-09
---

# M11l Plan — Command blocks

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[63 - Milestone 11l - Command Blocks]] (incl. "Settled before planning") |
| **Status** | 🟨 Designed and planned 2026-11-09; not built |
| **Tasks done** | 0 / 8 (Task 7, the release, is folded into the one release after M11m) |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | Yours (2026-11-08): Warp-style blocks per command and its output (collapse, copy, rerun), the riskiest, last, with its own careful design. Yours (2026-11-09): the one combined release ships after M11m. Everything else: AI's calls in note 63 (you skipped the design questions), including **collapse moved to "Not in M11l"**. |
| **Needs your eyes** | 👀 the 18-item on-screen check in note 63, after the combined release |

**Goal:** In zsh terminals fuxx finds each command through OSC 133 marks from its own injected integration, keeps
each command's record as a hidden hyperlink on the command row's first cell, and draws a gutter bar, a hairline
separator and a hover chip (Copy command, Copy output, Rerun), with ⇧⌘↑ / ⇧⌘↓ to jump and ⇧⌘C to copy output.

**Architecture:** Pure cores first, GPUI last. `shell_marks.rs` scans raw bytes for marks (Task 1); `blocks.rs`
splits `Processor::advance` at each mark and writes / reads `fuxx-block:` tags on a `Term` (Task 1); every Term that
parses child output (local reader, `show_local`, the window's replica, the service's session) goes through
`blocks::advance` (Task 2); `shell_integration.rs` installs the zsh scripts and adds `ZDOTDIR` to a login shell's
environment, behind the new `command-blocks` setting (Task 3); `block_view.rs` + `terminal_view.rs` paint the
overlay and chip (Task 4) and run the actions, keys, palette rows and Help rows (Task 5).

**Tech Stack:** `alacritty_terminal` 0.26.0 (`Cell::set_hyperlink` / `hyperlink()`, `Hyperlink::new`, `Grid`
indexing by `Line`/`Column`, `grid().cursor.point`, `Term::bounds_to_string`, `TermMode::ALT_SCREEN` /
`BRACKETED_PASTE`) and `vte` 0.15 (`Processor::advance`, `Processor::sync_bytes_count`), all **checked in the crate
sources in `~/.cargo/registry` on 2026-11-09** (Context7 has only alacritty's docs; it confirms the 0.26.0 `Config`
and nothing on OSC 133); GPUI at the pinned rev (`1d2a4b3f…`), only APIs already used in this repo (`canvas`,
`paint_quad`, `insert_hitbox`, `on_mouse_event`, absolutely positioned `div`s, `cx.background_spawn`, timers,
`write_to_clipboard`). zsh ≥ 5.1 for the scripts. **No new crates.**

**Spec:** [[63 - Milestone 11l - Command Blocks]]

## Global Constraints

- **No new crates** (`[dependencies]` unchanged). **One new setting**, `command-blocks = on | off` (default `on`),
  and nothing else persisted. **Three new key bindings** (context `Terminal`): `cmd-shift-up` → `terminal::PrevCommand`,
  `cmd-shift-down` → `terminal::NextCommand`, `cmd-shift-c` → `terminal::CopyCommandOutput`; `CopyCommand`,
  `CopyCommandAndOutput`, `RerunCommand` have no keys. ⌘↑ / ⌘↓ stay `ScrollPageUp` / `ScrollPageDown`. Scripting
  API, MCP, CLI, Activity and `StatusTracker` unchanged.
- **Never edit the user's dotfiles.** fuxx writes only `~/Library/Application Support/fuxx/shell-integration/zsh/.zshenv`
  and `…/zsh/fuxx-integration.zsh`, at startup, only when missing or different.
- **Injection rule:** only when the session runs the login shell (`program` is `None`), `$SHELL`'s file name is
  exactly `zsh`, `command-blocks` is on, and the scripts were installed. Adds `ZDOTDIR=<dir>/zsh`, plus
  `FUXX_ZSH_ZDOTDIR=<yours>` only when fuxx's own environment has `ZDOTDIR`. Nothing else in the environment changes.
- **Marks** (`ESC ]133;` + letter, BEL or `ESC \` terminated): `A`, `B`, `C` with optional `;cmdline_url=<%-encoded>`,
  `D` with optional `;<exit>` (i32). Other letters / keys ignored. A mark over `MARK_CAP = 8192` bytes is dropped.
- **Tag** = `Hyperlink::new(Some("fuxx-block-<n>"), uri)` on column 0 of the command row's logical-line start; `uri`
  = `fuxx-block:?v=1&s=<prompt|running|done>&p=<0..=8>[&c=<n>][&o=<n>][&x=<i32>][&cmd=<%>][&t=1]`, bytes
  0x21–0x7E only, never `;`. `CMD_CAP = 2048` bytes of UTF-8 (longer → no `cmd`, `t=1`), `PROMPT_LINES_CAP = 8`,
  `C_SCAN_ROWS = 500` (C looks this far up), D scans to the top of scrollback. Counts are **logical lines**.
- **No tag is written** on the alternate screen, while `processor.sync_bytes_count() > 0`, or when a scan finds no
  matching tag. A second C only fills a missing `cmd`; a D on a non-`running` tag is ignored.
- **`links.rs`** ignores hyperlinks whose URI starts with `fuxx-block:` (the plain-URL search still runs on that cell).
- **Look** (`block_view.rs`): `GUTTER_BAR = 2.0`, `GUTTER_BAR_ACTIVE = 3.0` (hover and chosen), radius 1, bar centre
  `GUTTER_INSET = 8.0` left of the grid origin (`6.0` in a split); separator 1 pt `card_border` at the top edge of a
  block's first prompt row and of the live prompt's (not on the first line of scrollback; not for older `s=prompt`
  tags); chosen block: `accent` bar over the whole block. Chip: `CHIP_HEIGHT = 24.0`, `CHIP_GAP = 4.0` right of the
  gutter, `panel` fill, 1 pt `panel_border`, radius 6, 8 pt side padding, `text::META` buttons; outcome + ` · ` +
  first command line cut to `CHIP_CMD_CHARS = 40` with `…`; `CHIP_NARROW = 220.0` grid width → buttons `Cmd`,
  `Output`, `Rerun`; hidden `CHIP_HIDE_MS = 150` after the pointer leaves gutter and chip. Nothing animates.
- **Colours** (`UiColors`, fitted per theme): `block_ok = ensure_contrast(palette[8], background, MARK_MIN)`,
  `block_failed = ensure_contrast(palette[1], background, MARK_MIN)`, `block_running = ensure_contrast(ring_working,
  background, MARK_MIN)`. Chip `exit N` (N ≠ 0) in `ensure_contrast(block_failed, panel, TEXT_MIN)`.
- **Copy (exact):** chip outcome `exit {n}` / `running` / (nothing when unknown); buttons `Copy command`, `Copy output`,
  `Rerun` (narrow: `Cmd`, `Output`, `Rerun`). Palette rows `Previous Command`, `Next Command`, `Copy Command Output`,
  `Copy Command`, `Copy Command and Output`, `Rerun Command` (group Terminal, place Terminal, keywords `block prompt
  jump` / `block copy` / `block run again`). Help rows (Terminal) `Previous command`, `Next command`, `Copy command
  output`. Setting row: `Command blocks`, sub-line `Marks each command in zsh. New terminals only.` Copy command and
  output = `"$ " + cmd + "\n" + output`.
- **Rerun** never sends `\r`; allowed only when `prompt_ready`, the cursor is at the recorded input start, not on the
  alternate screen, `cmd` present, no control characters but `\t` / `\n`, and multi-line only with
  `BRACKETED_PASTE`; sent via `TerminalView::paste`.
- **Nothing blocks the UI thread:** output text is built in `cx.background_spawn` (the Term lock held only to read);
  per-frame work is the viewport + 8 rows, plus a column-0 scan of scrollback for the chosen block only.
- **Contrast** on every built-in theme (`theme::every_theme`): the three block colours and `accent` ≥ 3:1 on the
  background; chip text ≥ 4.5:1 on `panel` and `panel_hover`; `card_border` ≥ 1.4:1 on the background.
- Every visual change has ui-shots in **Catppuccin Mocha and Latte**; new scenes are named `command-blocks*`.
  Existing tests keep passing; no existing scene changes pixels (shot terminals are `cat`, no marks).
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo clippy --all-targets` (both feature sets:
  default and `--features ui-shots`), `cargo fmt --check`, `cargo run --features ui-shots -- __ui-shots [filter]`,
  `scripts/bundle-mac.sh --app-only`.

## Review Focus

1. **Your zsh setup still starts exactly as before.** A `.zshenv` that sets `ZDOTDIR`, a custom `ZDOTDIR` with its
   own `.zshrc`, powerlevel10k / starship rebuilding `PS1` in `precmd`, iTerm2's integration also sourced, a nested
   `zsh`. Expected: your files run once each in the usual order, `ZDOTDIR` inside the shell equals yours (or is
   unset), marks keep coming after theme refreshes, duplicates change nothing. (Task 3:
   `real_zsh_keeps_your_zdotdir_and_files`, `a_theme_that_rebuilds_ps1_still_gets_marks`; Task 1:
   `duplicate_marks_are_idempotent`.)
2. **Reflow, the scrollback cap and clears.** Narrow then widen over wrapped commands; output past 10,000 lines;
   `clear`, ⌃L, `reset`. Expected: tags follow their lines; an evicted tag removes only its block; `ESC[2J` keeps
   tags in scrollback, `ESC[3J` / `ESC c` drop them; nothing panics on counts that run off the grid. (Task 1:
   `tags_survive_reflow`, `an_evicted_tag_removes_only_its_block`, `clears_keep_or_drop_tags`,
   `counts_past_the_grid_are_clamped`.)
3. **The alternate screen and agent TUIs.** vim / htop / tmux between marks; Claude Code drawing inline; marks inside
   a synchronized update. Expected: nothing tagged or drawn on the alternate screen; the inline agent is one running
   block; sync-buffered marks dropped. (Task 1: `marks_on_the_alt_screen_are_ignored`,
   `marks_inside_a_sync_update_are_ignored`; Task 4: scene `command-blocks-alt-screen`.)
4. **Rerun safety.** A faked block from `cat`-ed text with ESC or `\x1b[201~` in its command, a multi-line command
   with bracketed paste off, text already typed at the prompt, a reattached session. Expected: never a `\r`,
   refused on control characters / unbracketed multi-line / typed input / unknown prompt state. (Task 5:
   `rerun_refusals`; Task 2: `a_reattached_replica_is_not_prompt_ready`.)
5. **Persistent sessions.** The service tags, the snapshot carries tags, the replica keeps tagging after reattach.
   Expected: blocks before and after a reattach; a `D` after reattach finishes the block from the snapshot. (Task 2:
   `snapshot_round_trip_keeps_blocks`, `the_service_and_replica_tag_alike`.)

---

### Task 1: Marks and tags (pure core)

**Files:** Create `src/shell_marks.rs`, `src/blocks.rs`; Modify `src/main.rs` (`mod shell_marks; mod blocks;`).

**Interfaces — Produces:**
- `shell_marks.rs`: `#[derive(Clone, Debug, PartialEq, Eq)] pub enum Mark { PromptStart, CommandStart,
  OutputStart { cmd: Option<String>, truncated: bool }, Done { exit: Option<i32> } }`; `pub const MARK_CAP: usize =
  8192;` `pub struct MarkScanner` with `pub fn new() -> Self` and `pub fn feed(&mut self, bytes: &[u8]) ->
  Vec<(usize, Mark)>` (offset = index just past the terminator within `bytes`; a mark split across calls is
  reported in the call that ends it, at that call's offset).
- `blocks.rs`:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum BlockState { Prompt, Running, Done }`;
    `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Tag { pub id: String, pub state: BlockState, pub prompt_lines:
    u8, pub cmd_lines: Option<u32>, pub out_lines: Option<u32>, pub exit: Option<i32>, pub cmd: Option<String>,
    pub truncated: bool }`; `pub fn encode(tag: &Tag) -> Hyperlink`; `pub fn decode(link: &Hyperlink) -> Option<Tag>`;
    `pub const URI_PREFIX: &str = "fuxx-block:";` `CMD_CAP`, `PROMPT_LINES_CAP`, `C_SCAN_ROWS` per Global Constraints.
  - `pub struct Tagger` (`pub fn new() -> Self`; state `Unknown | Prompt { .. } | Running` per note 63 Settled 4;
    `pub fn prompt_ready(&self) -> bool`; `pub fn input_start(&self) -> Option<Point>`).
  - `pub fn advance<L: EventListener>(processor: &mut Processor<StdSyncHandler>, term: &mut Term<L>, tagger: &mut
    Tagger, scanner: &mut MarkScanner, bytes: &[u8])`.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Outcome { Ok, Failed(i32), Unknown, Running }`;
    `#[derive(Clone, Debug, PartialEq)] pub struct Block { pub id: String, pub outcome: Outcome, pub prompt_top: Line,
    pub cmd_row: Line, pub output: Option<(Line, Line)>, pub cmd: Option<String> }` (output `None` when empty);
    `pub fn list<L>(term: &Term<L>) -> Vec<Block>` (running + done, oldest first, empty on the alt screen);
    `pub fn live_prompt<L>(term: &Term<L>) -> Option<Line>` (newest `s=prompt` tag's prompt top, if it is the newest tag);
    `pub fn output_text<L>(term: &Term<L>, id: &str) -> Option<String>`.

- [ ] **Step 1: failing tests** (`shell_marks`):
  - `reads_the_four_marks`: `feed(b"\x1b]133;A\x07x\x1b]133;B\x1b\\")` → `[(8, PromptStart), (18, CommandStart)]`;
    `b"\x1b]133;C;cmdline_url=echo%20hi\x07"` → `OutputStart { cmd: Some("echo hi"), truncated: false }`;
    `b"\x1b]133;D;1\x07"` → `Done { exit: Some(1) }`; `b"\x1b]133;D\x07"` → `Done { exit: None }`.
  - `a_mark_split_at_every_byte_is_found_once`: for each split point of `b"ab\x1b]133;C;cmdline_url=ls\x07cd"`, two
    feeds yield exactly one `OutputStart { cmd: Some("ls"), .. }`, reported in the second feed when the split is
    inside the mark.
  - `ignores_other_letters_and_bad_input`: `133;P`, `133;Z`, `1330;A`, `133;D;x` (→ `Done { exit: None }`), a bad
    `%G1` (→ `cmd: None`) — none panics; an `ESC` inside a mark abandons it.
  - `drops_an_overlong_mark`: `C;cmdline_url=` + 9,000 bytes + BEL → nothing; the next `A` still found.
- [ ] **Step 2: failing tests** (`blocks`, on a real `Term` with a no-op listener, 20 cols × 6 rows, history 50 via
  `Config { scrolling_history: 50, ..term_config() }`, fed with `advance`):
  - `tag_round_trips`: `decode(&encode(&t)) == Some(t)` for a full tag and one with `cmd` containing `; & % é \n`;
    the URI has only 0x21–0x7E and no `;`.
  - `one_command_is_one_block`: `A "$ " B`, typed `"ls"`, `"\r\n"`, `C;cmdline_url=ls`, `"a\r\nb\r\n"`, `D;0`, `A "$ " B`
    → `list` = one block: `outcome Ok`, `cmd Some("ls")`, `prompt_top == cmd_row`, output = the two lines `a`, `b`;
    `output_text == Some("a\nb")`; `live_prompt` is the last line's row.
  - `a_two_line_prompt_and_a_wrapped_command`: prompt `"dir\r\n$ "` (p = 1), a 30-char command (wraps at 20 cols)
    → `prompt_top == cmd_row - 1`, output starts two rows below `cmd_row`.
  - `failed_and_unknown_exits`: `D;2` → `Failed(2)`; `D` → `Unknown`; C without D → `Running`, its output up to the
    cursor row.
  - `duplicate_marks_are_idempotent`: A,B,A,B → one prompt tag; C (no cmd) then C;cmdline_url=ls → `cmd Some("ls")`;
    D;0 then D;1 → `Ok`.
  - `tags_survive_reflow`: after `one_command_is_one_block` with a 30-char output line, `term.resize` to 10 cols
    then 40 → the same block id, cmd and `output_text`.
  - `an_evicted_tag_removes_only_its_block`: two blocks, then 60 lines of output in a third → `list` holds only the
    third; no panic.
  - `clears_keep_or_drop_tags`: after one block, `ESC[H ESC[2J` → still listed; `ESC[3J` → empty; another block then
    `ESC c` → empty.
  - `counts_past_the_grid_are_clamped`: a tag rewritten by hand with `o=999` → its output ends at the last row.
  - `marks_on_the_alt_screen_are_ignored`: `ESC[?1049h` + A,B,C,D + `ESC[?1049l` → no tags; a running block whose
    output uses the alt screen still finishes on D.
  - `marks_inside_a_sync_update_are_ignored`: `ESC[?2026h` + A + B → no tag; after `ESC[?2026l`, new marks tag.
  - `an_overlong_command_is_kept_without_text`: C with a 3,000-byte command → `cmd None`, `truncated true`.
  - `empty_prompts_are_not_blocks`: A,B,`\r\n`,A,B (no C) → `list` empty; `live_prompt` = the second.
  - `the_tagger_knows_the_prompt`: after B → `prompt_ready()` and `input_start() == Some(cursor)`; after C → false.
- [ ] **Step 3: run** `cargo test shell_marks blocks` → FAIL (undefined).
- [ ] **Step 4: implement** per Interfaces, Global Constraints and note 63 §1. `advance`: scan the chunk, then for
  each `(end, mark)` advance the processor over `bytes[from..end]` and apply the mark (skipped on the alt screen or
  with `sync_bytes_count() > 0`), then the rest. Logical lines: a row continues the one above when the row above's
  last cell has `Flags::WRAPLINE`. Ids: `fuxx-block-{n}`, counter seeded from `SystemTime` nanoseconds.
- [ ] **Step 5: run** `cargo test`, `cargo clippy --all-targets` (both feature sets), `cargo fmt --check` → green.
- [ ] **Step 6: commit** `feat(m11l): OSC 133 marks and command tags kept in the grid (pure)`.

### Task 2: Every terminal tags its output

**Files:** Modify `src/terminal.rs` (`Shared` gains `tagger: Tagger, marks: MarkScanner`; the local reader,
`show_local` and `Replica::apply`'s `Output` use `blocks::advance`; a reattach / `Resync` gives a fresh `Tagger`),
`src/daemon_session.rs` (`State` gains the same; `output` uses `blocks::advance`), `src/grid.rs` (`GridSnapshot.blocks`),
`src/links.rs` (skip `URI_PREFIX`), `src/vt_snapshot.rs` (test only).

**Interfaces — Consumes:** Task 1. **Produces:**
- `grid.rs`: `#[derive(Clone, Debug, PartialEq)] pub struct BlockView { pub id: String, pub outcome: Outcome, pub
  prompt: (i32, i32), pub output: Option<(i32, i32)>, pub chosen: bool }` (viewport rows, inclusive, may lie outside
  `0..rows`) and `pub blocks: Vec<BlockView>`, `pub live_prompt: Option<i32>` on `GridSnapshot` (empty / `None` on
  the alt screen; `grid::snapshot` leaves them empty).
- `TerminalCore`: `pub fn snapshot_with_blocks(&self, chosen: Option<&str>) -> GridSnapshot` (`snapshot()` +
  `blocks::visible` under the same lock: viewport + 8 rows below, the chosen id over the whole scrollback);
  `pub fn blocks(&self) -> Vec<Block>`; `pub fn block_output(&self, id: &str) -> Option<String>`;
  `pub fn block_cmd(&self, id: &str) -> Option<String>`; `pub fn prompt_ready(&self) -> bool`;
  `pub fn cursor_at_input_start(&self) -> bool`; `pub fn scroll_to_line(&self, line: Line)`
  (`display_offset = clamp(-line, 0, history_size)`).
- `blocks.rs`: `pub fn visible<L>(term: &Term<L>, chosen: Option<&str>) -> (Vec<BlockView>, Option<i32>)`.

- [ ] **Step 1: failing tests**
  - (`terminal`) `a_local_shell_s_marks_become_blocks`: spawn `sh -c` printing `\033]133;A\007$ \033]133;B\007` …
    `C;cmdline_url=ls` … `ok\n` … `D;0` … A/B → `core.blocks()` has one block `cmd Some("ls")`, `Ok`;
    `block_output == Some("ok")`; `prompt_ready()`.
  - `show_local_draws_blocks`: `show_local` with the same text → one block (screenshots rely on it).
  - `a_reattached_replica_is_not_prompt_ready` (scripted daemon, as `a_seq_gap_or_resync_reattaches_onto_a_fresh_replica`):
    snapshot frame built from a Term that ends at a prompt → `prompt_ready() == false`; after an `Output` with A,B →
    true.
  - `the_service_and_replica_tag_alike` (daemon test): a `DaemonSession` running the same script → its Term's
    `blocks::list` equals the replica's (ids may differ: compare outcome, cmd, rows).
  - (`vt_snapshot`) `snapshot_round_trip_keeps_blocks`: two blocks + live prompt, `encode` → fresh Term →
    `blocks::list` equal, `output_text` equal; then feed `C`/`D` for a running block encoded mid-command → it
    finishes on the rebuilt Term.
  - (`links`) `block_tags_are_not_links`: a tagged cell whose row text is `https://x.io` → `find` returns the plain
    URL, never a `fuxx-block:` link.
  - (`blocks`) `visible_rows_and_the_chosen_block`: 3 blocks with history scrolled so block 1 is above the viewport;
    `visible(term, Some(id1))` includes block 1 with `chosen: true` and negative rows; without `chosen`, block 1 is
    absent; on the alt screen both are empty.
- [ ] **Step 2: run** `cargo test terminal blocks vt_snapshot links daemon_session` → FAIL.
- [ ] **Step 3: implement** (the existing `OscDetector`, `CwdReports`, `ClearCounter` still see whole chunks).
- [ ] **Step 4: run** `cargo test`, clippy (both), fmt.
- [ ] **Step 5: commit** `feat(m11l): local, replica and service terminals tag commands; snapshots carry them`.

### Task 3: zsh integration and the `command-blocks` setting

**Files:** Create `src/shell_integration.rs`, `assets/shell-integration/zsh/.zshenv`,
`assets/shell-integration/zsh/fuxx-integration.zsh`; Modify `src/main.rs` (`mod`; `shell_integration::install`
before the first window), `src/workspace.rs` (`session_env` and `create_env` gain `login_shell: bool`;
`open_local` passes `program.is_none()`, `create_for` likewise; `scripting_env` unchanged), `src/workspace_daemon.rs`
(its `create_env` call), `src/appearance.rs` (`pub command_blocks: bool`, `command_blocks_from_entries`),
`src/preferences.rs` (`OnOff::CommandBlocks` in Sessions after `KeepRunning`, key `command-blocks`, label and
sub-line per Global Constraints), `src/ui_shots.rs` (the Preferences scenes' Sessions expectations).

**Interfaces — Produces:**
- `pub fn integration_dir() -> Option<PathBuf>` (`$HOME/Library/Application Support/fuxx/shell-integration`);
  `pub fn install(dir: &Path) -> io::Result<bool>` (writes the two files, `0o644`, only when missing or different;
  `Ok(true)` when it wrote); `pub fn installed() -> Option<&'static Path>` (set by `main` after a successful
  install); `pub fn env_for(shell: &Path, user_zdotdir: Option<&OsStr>, enabled: bool, dir: Option<&Path>) ->
  Vec<(String, String)>`.
- `Appearance.command_blocks` (read by Tasks 4–5).

- [ ] **Step 1: failing tests**
  - `env_for_zsh_only`: `/bin/zsh`, none, on, `Some("/i")` → `[("ZDOTDIR", "/i/zsh")]`; with `Some("/u/zd")` → also
    `("FUXX_ZSH_ZDOTDIR", "/u/zd")`; `/opt/homebrew/bin/zsh` → same; `/bin/bash`, `/usr/local/bin/fish`, `/bin/sh`,
    `/bin/zsh5` → `[]`; `enabled false` → `[]`; `dir None` → `[]`.
  - `session_env_injects_only_a_login_shell` (workspace): `login_shell: false` → no `ZDOTDIR` key added.
  - `install_writes_once`: temp dir → `Ok(true)`, files equal the `include_str!` texts; again → `Ok(false)`; a file
    changed by hand → `Ok(true)` and restored.
  - `real_zsh_keeps_your_zdotdir_and_files` (spawns `/bin/zsh -il` through `TerminalCore::spawn`, `HOME` = temp with
    `.zshenv` `export T_ENV=1` and `.zshrc` `PS1='%# '; export T_RC=1`, the env from `env_for`): type
    `echo hi\r`, `false\r`, `print -r -- "[$ZDOTDIR][$T_ENV][$T_RC][${FUXX_ZSH_ZDOTDIR-unset}]"\r` → `core.blocks()`
    outcomes `[Ok, Failed(1), Ok]`, cmds `["echo hi", "false", …]`, the third's output `[][1][1][unset]`. Second
    run with `ZDOTDIR` = temp `zd` holding the `.zshrc` (and passed as `user_zdotdir`) → output `[<zd>][1][1][unset]`.
  - `a_theme_that_rebuilds_ps1_still_gets_marks`: `.zshrc` adds a `precmd` that sets `PS1="%# $RANDOM "` each time →
    three commands give three blocks and `prompt_ready()` after each.
  - `command_blocks_setting`: `command-blocks = off` → false; missing → true; `"maybe"` → true plus the warning
    `command-blocks "maybe": expected on or off`; `OnOff::CommandBlocks.key() == "command-blocks"`.
  - ui-shots (Mocha, Latte) `preferences` Sessions: the `Command blocks` row and sub-line present; toggling writes
    `command-blocks = off`.
- [ ] **Step 2: run** `cargo test shell_integration workspace appearance preferences terminal` → FAIL.
- [ ] **Step 3: implement.** `.zshenv` and `fuxx-integration.zsh` per note 63 §1 "Shell integration" (restore
  `ZDOTDIR`, source the user's `.zshenv`, interactive → source the integration; `preexec` → C with percent-encoded
  `$1` unless over 2,048 bytes; `precmd` → `D;$?` only after a `preexec`, re-add `%{…133;A…%}` / `%{…133;B…%}` to
  `PS1` when missing, move itself to the end of `precmd_functions` once; guard against double sourcing). `install`
  failure is logged once (`[fuxx] shell integration: …`) and leaves injection off.
- [ ] **Step 4: run** `cargo test`, clippy (both), fmt, `__ui-shots preferences`.
- [ ] **Step 5: commit** `feat(m11l): zsh shell integration via ZDOTDIR; the command-blocks setting`.

### Task 4: The overlay — gutter bars, separators, the chip

**Files:** Create `src/block_view.rs`; Modify `src/theme.rs` (`block_ok`, `block_failed`, `block_running` in
`UiColors` + `ui()`), `src/terminal_view.rs` (paint from `snapshot_with_blocks(chosen)`; the gutter geometry in an
`Rc<RefCell<Vec<GutterHit>>>` written in paint; `hovered_block`, `chosen_block: Option<String>`; the chip as an
absolute child of the root `div`; the hide timer), `src/main.rs` (`mod block_view`), `src/ui_shots.rs` (scenes).

**Interfaces — Consumes:** Task 2 (`BlockView`, `snapshot_with_blocks`, `block_cmd`), Task 3
(`Appearance.command_blocks`). **Produces:**
- `block_view.rs`: the constants of Global Constraints; `pub fn bar_color(outcome: Outcome, ui: &UiColors) -> Rgb`;
  `pub fn gutter_x(split: bool) -> f32`; `pub fn chip_outcome(outcome: Outcome) -> Option<String>`;
  `pub fn chip_command(cmd: &str) -> String` (first line, ≤ 40 chars + `…`); `pub fn chip_buttons(grid_width: f32)
  -> [&'static str; 3]`; `#[derive(Clone, Copy, Debug, PartialEq)] pub struct GutterHit { pub id_index: usize,
  pub top: f32, pub bottom: f32 }` (window y, with the id list alongside).
- `TerminalView`: `pub fn chosen_block(&self) -> Option<&str>`; `pub fn set_chosen_block(&mut self, id:
  Option<String>, cx)`; ui-shots: `pub fn hovered_block(&self) -> Option<&str>`.

- [ ] **Step 1: failing tests**
  - `block_inks_meet_the_floors` (block_view): per Global Constraints "Contrast" on every built-in theme.
  - `chip_texts`: `chip_outcome(Failed(1)) == Some("exit 1")`, `Ok` → `Some("exit 0")`, `Running` → `Some("running")`,
    `Unknown` → `None`; `chip_command("npm test\nsecond")  == "npm test"`; a 50-char command → 40 chars + `…`;
    `chip_buttons(219.0) == ["Cmd", "Output", "Rerun"]`, `chip_buttons(220.0) == ["Copy command", "Copy output",
    "Rerun"]`; `gutter_x(false) == 8.0`, `gutter_x(true) == 6.0`.
  - ui-shots (Mocha, Latte), one shot terminal whose `show_text` holds: block `echo hi` (exit 0), block `false`
    (exit 1, two-line prompt), block `sleep 9` (running, two output lines), then a live prompt:
    - `command-blocks`: a pixel at each block's command row, `gutter_x` left of the grid = `block_ok`, `block_failed`,
      `block_running`; a pixel on the separator row above block 2's prompt = `card_border`; the same column one row
      lower is the background; no separator above an older empty prompt (add one in the text); hover block 2's
      gutter → `hovered_block() == Some(id2)`, the chip's text includes `exit 1 · false`, `Copy command`,
      `Copy output`, `Rerun`, its left edge 4 pt right of the gutter; 200 ms after moving away it's still shown,
      after 200 ms + 150 ms it's gone; two frames 200 ms apart with the chip shown are pixel-identical (Animations on,
      Reduce motion on).
    - `command-blocks-alt-screen`: then `show_text("\x1b[?1049h…")` → no `block_*` pixel in the gutter, no separator;
      `\x1b[?1049l` → back.
    - `command-blocks-narrow`: the pane split to a 200 pt grid → chip buttons `Cmd`, `Output`, `Rerun`, the chip's
      right edge inside the pane; bars at `gutter_x(true)`.
    - `command-blocks-off`: `command_blocks = false` → no gutter or separator pixels; hover does nothing.
- [ ] **Step 2: run** `cargo test block_view`, `__ui-shots command-blocks` → FAIL.
- [ ] **Step 3: implement** (chip buttons render but do nothing until Task 5; a gutter press never reaches selection
  or mouse reporting; any output, typing or a grid press hides the chip).
- [ ] **Step 4: run** `cargo test`, clippy (both), fmt, `__ui-shots command-blocks`, the full `__ui-shots`.
- [ ] **Step 5: commit** `feat(m11l): gutter bars, separators and the hover chip for command blocks`.

### Task 5: Jump, copy, rerun — keys, chip buttons, palette, Help

**Files:** Modify `src/block_view.rs` (pure rules), `src/terminal_view.rs` (actions, `on_action`s, chip buttons,
gutter click), `src/main.rs` (actions and bindings), `src/help_shortcuts.rs` (three rows), `src/palette_items.rs`
(six rows), `src/ui_shots.rs` (scenes).

**Interfaces — Consumes:** Tasks 2 and 4. **Produces:**
- `terminal_view.rs`: `actions!(terminal, [Copy, Paste, ScrollPageUp, ScrollPageDown, PrevCommand, NextCommand,
  CopyCommandOutput, CopyCommand, CopyCommandAndOutput, RerunCommand])`.
- `block_view.rs`: `#[derive(Debug, PartialEq, Eq)] pub enum Jump { To(usize), Bottom, Stay }`;
  `pub fn jump(blocks: &[(Line, bool)], chosen: Option<usize>, up: bool) -> Jump` (`(prompt_top, visible_entirely)`
  oldest first); `pub fn target(blocks_len: usize, chosen: Option<usize>, newest_done: Option<usize>) -> Option<usize>`;
  `#[derive(Debug, PartialEq, Eq)] pub enum Refusal { NotAtPrompt, Typed, NoCommand, Control, MultiLineUnbracketed }`;
  `pub fn rerun_text(cmd: Option<&str>, prompt_ready: bool, at_input_start: bool, bracketed: bool, alt_screen: bool)
  -> Result<String, Refusal>`; `pub fn with_command(cmd: &str, output: &str) -> String` (`"$ {cmd}\n{output}"`).

- [ ] **Step 1: failing tests**
  - `jump_walks_the_blocks` (block_view): 3 blocks, none chosen, up → `To(2)`; chosen 2 up → `To(1)`; chosen 0 up →
    `Stay`; chosen 2 down → `Bottom`; none, down → `Stay`; no blocks → `Stay`.
  - `actions_target_the_chosen_or_newest_done`: `target(3, Some(1), Some(2)) == Some(1)`; `target(3, None, Some(1)) ==
    Some(1)`; `target(0, None, None) == None`.
  - `rerun_refusals`: `rerun_text(Some("ls"), true, true, true, false) == Ok("ls")`; `prompt_ready false` →
    `NotAtPrompt`; alt screen → `NotAtPrompt`; `at_input_start false` → `Typed`; `None` → `NoCommand`;
    `"ls\x1b[201~rm"` and `"a\x07"` → `Control`; `"a\tb"` → `Ok`; `"a\nb"` unbracketed → `MultiLineUnbracketed`,
    bracketed → `Ok`. Never a `\r` in any `Ok`.
  - `with_command_formats`: `with_command("ls", "a\nb") == "$ ls\na\nb"`.
  - (help_shortcuts) `block_keys_are_in_the_terminal_group`: rows `Previous command` `⇧⌘↑`, `Next command` `⇧⌘↓`,
    `Copy command output` `⇧⌘C`; the existing parity tests pass.
  - (palette_items) `every_bound_or_menu_action_is_in_the_palette_or_left_out` passes with the six actions;
    `block_commands_are_found`: `command` finds all six, `block` finds them, `run again` finds `Rerun Command`;
    place Terminal.
  - ui-shots (Mocha, Latte), the Task 4 terminal plus 40 lines of filler output in block 1 so it scrolls:
    - `command-blocks-jump`: ⇧⌘↑ → `chosen_block() == Some(id3)`; again → `id2`, the display offset puts block 2's
      first prompt row at viewport row 0 and its full-height bar pixel = `accent`; ⇧⌘↓ ×2 → `None`, offset 0; ⌘↑
      still scrolls a page (offset grows by the row count); typing a key clears the choice.
    - `command-blocks-copy`: nothing chosen, ⇧⌘C → clipboard (after the background task) = block 2's output text
      (newest done); chip of block 1 → Copy command → `echo hi`; palette `Copy Command and Output` on block 1 →
      `$ echo hi\nhi`; a click on block 3's bar → chosen 3, no scroll.
    - `command-blocks-rerun`: live prompt ready → chip Rerun on block 1 → the shot terminal's screen shows `echo hi`
      after the live prompt (the PTY's echo of what was sent) and the cursor stays on that row (no Return); then the
      chip shows Rerun in `text_faint` (the cursor left the input start) and clicking it changes nothing on screen.
- [ ] **Step 2: run** `cargo test block_view help_shortcuts palette_items`, `__ui-shots command-blocks` → FAIL.
- [ ] **Step 3: implement** (Copy output / command and output built in `cx.background_spawn` from a cloned core;
  all block actions no-op when `command_blocks` is off, on the alt screen, or with no target; ⇧⌘↑ / ⇧⌘↓ scroll with
  `scroll_to_line`, at once).
- [ ] **Step 4: run** `cargo test`, clippy (both), fmt, the full `__ui-shots`, `scripts/bundle-mac.sh --app-only`.
- [ ] **Step 5: commit** `feat(m11l): jump between commands, copy command and output, rerun at the prompt`.

### Task 6: Merge and push (pre-authorized, autonomous program)

- [ ] Full gate on the branch: `cargo test`, `cargo clippy --all-targets` (both feature sets), `cargo fmt --check`,
  full `__ui-shots` green; a whole-branch review against note 63 and this plan's Review Focus; then merge
  `m11l-command-blocks` into `master` and push.

### Task 7: Release — folded into the one release after M11m

- [ ] ~~Version bump, `chore(release): …`, tag, watch the Release run~~ Skipped by ruling: the one combined release
  (M11i–M11m) ships after M11m (your rule, 2026-11-09: "push the release after performance milestone").
- [ ] You: the 18-item on-screen checklist in note 63 (after the combined release).

### Task 8: Docs

- [ ] Note 63: status, an "M11l as built" section (what the build settled or changed), the checklist's state. This
  plan (ticks, header). The roadmap (`40 - Roadmap.md`). The Plan Tracker. The Progress Log. Note 56 (Help: the three
  Terminal rows). `STRUCTURE.md` (`shell_marks.rs`, `blocks.rs`, `shell_integration.rs`, `block_view.rs`,
  `assets/shell-integration/`). `README.md` (command blocks, ⇧⌘↑ / ⇧⌘↓ / ⇧⌘C, the zsh integration and
  `command-blocks = off`).
- [ ] Commit `docs(m11l): …`.
