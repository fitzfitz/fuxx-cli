---
tags: [development, plan, milestone]
milestone: M11k
status: planned
updated: 2026-11-09
---

# M11k Plan — First-run screen (Welcome card)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[62 - Milestone 11k - First-Run Screen]] (incl. "Settled before planning") |
| **Status** | ⬜ Planned 2026-11-09 |
| **Tasks done** | 0 / 7 |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | Yours (2026-11-08): a first-run screen — pick a folder, start an agent, learn three shortcuts. Everything else: AI's calls in note 62 (you skipped the design questions). |
| **Needs your eyes** | 👀 Task 6 (folded into the one release after M11l), then the 15-item on-screen check in note 62 |

**Goal:** On a first run a Welcome card over the first terminal lets you pick a folder, start an agent preset (marked
by whether it's installed) or a plain terminal there, and shows ⌘K, ⌘N, ⇧⌘U; Help → Welcome to fuxx reopens it.

**Architecture:** Pure cores first, GPUI second. `restore.rs` gains the `welcomed` state; `agent_detect.rs` finds a
preset's program on the login shell's `PATH` (pure, plus one blocking call run on the background executor);
`welcome_items.rs` holds the card's pure rules (rows, the default choice, the launch, the pick's effect, copy)
(Task 1). `welcome.rs` is the overlay view and its `Workspace` glue (Task 2), the pick and the first-run hook
(Task 3), then the Help menu item and the palette entry (Task 4).

**Tech Stack:** GPUI at the pinned rev (`1d2a4b3f…`), only APIs already used in this repo (`cx.prompt_for_paths`,
`on_key_down`, `occlude`, `ScrollHandle`/`track_scroll`, `cx.background_spawn`); `std::fs` + `PermissionsExt` for the
`PATH` check. **No new crates** (so no Context7 look-up: no new crate API).

**Spec:** [[62 - Milestone 11k - First-Run Screen]]

## Global Constraints

- **No new crates** (`[dependencies]` unchanged), **no new settings**, **no new key bindings** (Help's list and its
  parity test unchanged), no change to the scripting API, MCP or the CLI. The only new persisted thing is
  `SavedState.welcomed` in `sessions.json`.
- **Nothing blocks the UI thread:** `workspace_ssh::login_env()` (≤ 3 s, `LOGIN_ENV_CAP`) and the `PATH` scan run only
  inside `cx.background_spawn`. `PATH_CAP = 64` folders.
- **State:** `#[serde(default = "already_welcomed")] pub welcomed: bool` (→ `true` for files without it);
  `SavedState::default().welcomed == false`; `Workspace.welcomed` from `read_saved_quietly().is_some_and(|s|
  s.welcomed)` in `new`, `true` in `blank` (so `for_shots`). Shown only from `open_first`'s callback, after
  `finish_restoring`, when `!self.welcomed` and the first terminal opened; `open_welcome` sets `welcomed = true` and
  calls `save()` before showing.
- **Detection:** program = first word of the command after leading `NAME=value` words (name `[A-Za-z_][A-Za-z0-9_]*`),
  one layer of matching `'…'` or `"…"` removed; a command whose program starts with `$`, `(` or `` ` `` or is empty →
  `Unknown`. A program containing `/` is checked as that path (absolute only; relative → `Unknown`). Found = a
  regular file with any execute bit in one of the first 64 non-empty absolute `PATH` entries. No login env, or no
  `PATH` in it → all `Unknown`. Nothing is executed but the existing `login_env` read.
- **Rows** (`Row`): `Folder`, `Preset(i)` for each preset (≤ 9, in order), `Plain`. ↓/↑ round. Default row
  (`default_row`): first `Yes`; else if every preset is `No` (or there are none) → `Plain`; else `Preset(0)`.
  Detection moves the choice to `default_row` only while `moved == false`.
- **Pick effect** (`pick_effect`): `FirstRun` + `Plain` + folder is home → `JustClose`; `FirstRun { over }` with
  `over` alive and alone (`model.terminal_tab(over)` → `model.locate` → `tabs_of(ws).len() == 1`) → `Open {
  close_after: Some(over) }`; else `Open { close_after: None }`. Launch (`start_launch`): `project = None` when the
  folder is home, else `Some(folder)`; `Preset(p)` → `preset: Some(p), run_preset: true`; `Plain` → neither; always
  `At::New` via `open_session_then`; `close_after` is closed with `close_at` in its callback only when the new id is
  `Some`.
- **Run order on a pick:** `self.welcome = None`, `focus_active`, then the launch.
- **Closes on:** Esc, Skip, a scrim click, a pick, another overlay (`close_overlays`). **Not** on focus loss.
- **Copy exactly:** title `Welcome to fuxx`; subtitle `A terminal for running AI coding agents.`; headings `Pick a
  folder`, `Start an agent`, `Three shortcuts` (rendered uppercase); `Choose\u{2026}`; `Plain terminal`;
  `Checking\u{2026}`; `Not found on your PATH`; `No agent presets set.`; shortcut lines `Command palette \u{2014} do
  anything` (⌘K, `workspace::OpenPalette`), `New workspace for another agent` (⌘N, `workspace::NewWorkspace`), `Go to
  the agent that needs you` (⇧⌘U, `workspace::NextNeedingYou`); hint `Enter start \u{b7} \u{2191}\u{2193} choose
  \u{b7} Esc skip`; buttons `Skip`, `Start {preset name}`, `Open Terminal`. Help item `Welcome to fuxx`
  (`help::ShowWelcome`); `Show Welcome Tips` renamed `Show Tips Card` (action unchanged).
- **Look** (constants in `welcome.rs`): `CARD_WIDTH = 520.0` on `overlay_panel::shell`, inner padding
  `CARD_PADDING = 20.0`; centred both ways in a layer from `size::TOP_BAND` (38) to the bottom with `EDGE = 16.0`
  padding; scrim `ui.shadow` at `SCRIM_ALPHA = 0.35`, `occlude`, mouse down → Skip. Title 18 pt semibold `text`;
  subtitle `text::NAME` `text_dim`; headings `text::META` `text_faint`, 14 pt above, 6 below. Rows `ROW = 28.0`, 8 pt
  side padding, radius 6, 2 pt apart; chosen `row_active` + name `FontWeight::MEDIUM`; hover `panel_hover`; status
  word `text::META` `text_faint`; keys right `text_dim`. Folder field 28 pt `panel_hover`, radius 6, `home_relative`
  text start-cut; Choose… 24 pt, `panel_border` edge. Keycaps `KEYCAP = 22.0` tall, min 28 wide, 6 pt padding,
  radius 5, `panel_hover` fill, 1 pt `panel_border`, 12 pt `text` tabular; line `text_dim`, 10 pt after the widest
  cap. Footer: 1 pt `panel_border`, hint `text::META` `text_faint`; Skip no fill `text_dim`; Start `accent` fill,
  `on_accent` text, 13 pt medium, 28 tall, 12 side padding, radius 6. Card height ≤ window − 38 − 16; only the agent
  list scrolls.
- **Contrast:** on every built-in theme (`theme::every_theme`), `text`, `text_dim`, `text_faint` ≥ `TEXT_MIN` (4.5) on
  `panel`, `panel_hover`, `row_active`, and `on_accent` ≥ 4.5 on `accent`.
- **Motion:** nothing in the card animates (no spinner; opens/closes at once).
- Every visual change has ui-shots in **Catppuccin Mocha and Latte**. Existing tests keep passing; scenes whose
  pixels or flow change are updated in the task that changes them (named per task).
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo clippy --all-targets` (both feature sets:
  default and `--features ui-shots`), `cargo fmt --check`, `cargo run --features ui-shots -- __ui-shots [filter]`,
  `scripts/bundle-mac.sh --app-only`.

## Review Focus

1. **Upgrade versus first run.** An existing `sessions.json` without `welcomed` (every current user), a restored
   launch, `session-restore = off` with a file, a missing or unreadable file, persistence on. Expected: only a missing
   or unreadable file (or `welcomed: false`) shows the card, once. (Task 1: `an_older_sessions_file_counts_as_welcomed`,
   `welcome_due_only_without_a_record`; Task 3: scene `welcome-first-run`.)
2. **Detection that is slow, wrong or never answers.** A login shell that hangs (3 s cap), fish, no `PATH`, an alias,
   `FOO=1 claude`, a quoted path with a space, `$(…)`, a directory named `claude`, a non-executable file. Expected:
   the card is fully usable while `Checking…`, Enter starts the chosen row, wrong answers only change a label. (Task 1:
   the `agent_detect` tests; Task 2: scene `welcome-checking`.)
3. **Replacing the first terminal.** The new one fails to open; the first one isn't alone any more (a script added a
   tab); the first one was closed by a script; persistence on (async open). Expected: the first is closed only after
   the new one exists and only if alone; fuxx never quits in between. (Task 1: `pick_effect_*`; Task 3: scene
   `welcome-start` stages b and c.)
4. **Focus round trips.** Esc, Skip, a scrim click, ⌘K replacing the card, the folder panel taking and giving back
   the keyboard. Expected: the card never closes on focus loss; after it closes the terminal (or focused tab) types.
   (Task 2: scene `welcome-keys`; Task 3: `welcome-folder` via the test hook; the native panel itself is on-screen
   check 2.)
5. **Small windows and long content.** Nine presets, a 120-character folder, a 480 × 420 window. Expected: the card
   fits with 16 pt edges below the top band, the agent list scrolls with the chosen row in view, the folder is cut at
   its start. (Task 2: scene `welcome-narrow`.)

---

### Task 1: First-run state, agent detection and the card's rules (pure)

**Files:** Create `src/agent_detect.rs`, `src/welcome_items.rs`; Modify `src/restore.rs` (`welcomed`,
`already_welcomed`, `welcome_due`; every `SavedState { .. }` literal in the crate gains the field), `src/workspace.rs`
(`pub(crate) welcomed: bool`: `true` in `blank`, read in `new` per Global Constraints, written by `saved_state`),
`src/main.rs` (`mod agent_detect; mod welcome_items;`).

**Interfaces — Produces:**
- `restore.rs`: `pub welcomed: bool` on `SavedState`; `fn already_welcomed() -> bool { true }`;
  `pub fn welcome_due(saved: Option<&SavedState>) -> bool`.
- `agent_detect.rs`: `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Found { Checking, Yes, No, Unknown }`;
  `pub const PATH_CAP: usize = 64;` `pub fn program(command: &str) -> Option<String>` (None → Unknown);
  `pub fn on_path(program: &str, path: &OsStr, is_exec: impl Fn(&Path) -> bool) -> bool`;
  `pub fn is_executable_file(path: &Path) -> bool`; `pub fn find(commands: &[String], path: Option<&OsStr>,
  is_exec: impl Fn(&Path) -> bool) -> Vec<Found>` (pure core); `pub fn check(commands: Vec<String>) -> Vec<Found>`
  (blocking: `workspace_ssh::login_env()`'s `PATH`, then `find` with `is_executable_file`).
- `welcome_items.rs`: `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Origin { FirstRun { over: u64 }, Help }`;
  `#[derive(Clone, Debug, PartialEq, Eq)] pub enum Choice { Preset(Preset), Plain }`;
  `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Row { Folder, Preset(usize), Plain }`;
  `pub fn rows(presets: usize) -> Vec<Row>`; `pub fn step(rows: &[Row], at: Row, down: bool) -> Row`;
  `pub fn default_row(found: &[Found]) -> Row`; `pub fn start_launch(folder: &Path, home: Option<&Path>, choice:
  &Choice) -> Launch`; `#[derive(Debug, PartialEq, Eq)] pub enum Effect { JustClose, Open { close_after: Option<u64> } }`;
  `pub fn pick_effect(origin: Origin, over_alone: bool, choice: &Choice, folder_is_home: bool) -> Effect`;
  `pub fn start_label(choice: &Choice) -> String`; `pub fn status_word(found: Found) -> Option<&'static str>`;
  `pub const SHORTCUTS: [(&str, &str); 3]` (action name, line) and the copy constants of Global Constraints.

- [ ] **Step 1: failing tests**
  - (`restore`) `an_older_sessions_file_counts_as_welcomed`: `decode(r#"{"version":2}"#).unwrap().welcomed == true`;
    `a_fresh_state_is_not_welcomed`: `!SavedState::default().welcomed`; `welcomed_survives_a_round_trip`:
    `decode(&encode(&SavedState { welcomed: false, ..Default::default() })).unwrap().welcomed == false`;
    `welcome_due_only_without_a_record`: `welcome_due(None)`, `welcome_due(Some(&default))`, not
    `welcome_due(Some(&SavedState { welcomed: true, .. }))`.
  - (`agent_detect`) `program_takes_the_first_word`: `"claude"` → `claude`; `"claude --continue \"fix: it\""` →
    `claude`; `"FOO=1 BAR=x aider --model s"` → `aider`; `"'/opt/my tools/a' --f"` → `/opt/my tools/a`; `""`,
    `"   "`, `"$(which x)"`, `"(cd x; y)"`, `` "`x`" `` → `None`; `"FOO=1"` → `None`.
  - `on_path_finds_an_executable`: a temp dir `d` with an executable file `claude`, a non-executable file `codex`, a
    directory `opencode`; `PATH = "{d}:relative::/nope"` → `on_path("claude")` true, `codex` false, `opencode`
    false (real `is_executable_file`); `PATH` of 70 entries with `d` at 66th → false (`PATH_CAP`).
  - `a_program_with_a_slash_is_a_path`: `on_path("{d}/claude", "", …)` true; `on_path("bin/claude", …)` false.
  - `find_marks_each_command`: `find(&["claude", "codex", "$(x)"], Some(path), …)` → `[Yes, No, Unknown]`;
    `find(&["claude"], None, …)` → `[Unknown]`.
  - (`welcome_items`) `rows_are_folder_presets_then_plain`: `rows(3)` = `[Folder, Preset(0), Preset(1), Preset(2),
    Plain]`; `rows(0)` = `[Folder, Plain]`. `step_wraps`: from `Plain` down → `Folder`; from `Folder` up → `Plain`.
  - `default_row_picks_the_first_found`: `[Checking; 3]` → `Preset(0)`; `[No, Yes, Yes]` → `Preset(1)`; `[No, No]` →
    `Plain`; `[Unknown, No]` → `Preset(0)`; `[]` → `Plain`.
  - `start_launch_leaves_home_without_a_folder`: folder = home, `Plain` → `project None, preset None, run_preset
    false`; folder `/w/app`, `Preset(codex)` → `project Some("/w/app"), preset Some(codex), run_preset true`.
  - `pick_effect_replaces_only_an_alone_first_terminal`: `(FirstRun{over:1}, true, Plain, true)` → `JustClose`;
    `(FirstRun{over:1}, true, Preset, true)` → `Open{Some(1)}`; `(FirstRun{over:1}, true, Plain, false)` →
    `Open{Some(1)}`; `(FirstRun{over:1}, false, Preset, false)` → `Open{None}`; `(Help, true, Plain, true)` →
    `Open{None}`.
  - `labels_follow_the_choice`: `start_label(Preset(claude)) == "Start claude"`, `start_label(Plain) == "Open
    Terminal"`; `status_word(Checking) == Some("Checking…")`, `No` → `Some("Not found on your PATH")`, `Yes` and
    `Unknown` → `None`.
- [ ] **Step 2: run** `cargo test welcome_ agent_detect restore` → FAIL (undefined).
- [ ] **Step 3: implement** per Interfaces and Global Constraints (`is_executable_file`: `metadata().is_file() &&
  mode & 0o111 != 0`).
- [ ] **Step 4: run** `cargo test`, `cargo clippy --all-targets` (both feature sets), `cargo fmt --check` → green.
- [ ] **Step 5: commit** `feat(m11k): first-run state, agent detection and the welcome's rules`.

### Task 2: The Welcome card

**Files:** Create `src/welcome.rs`; Modify `src/workspace.rs` (`Overlay::Welcome` in `overlays!`, `welcome:
Option<Entity<Welcome>>`, `agents_frozen: bool` (true in `for_shots`), `close_overlays`, the
render's scrim layer, `open_welcome`, `on_welcome_event`), `src/pane_browser.rs` (`Overlays.welcome`),
`src/workspace_browser.rs` (`overlays()` sets it), `src/palette.rs` (`binding_table` → `pub(crate)`), `src/main.rs`
(`mod welcome`), `src/ui_shots.rs` (scenes below; the `overlays` arm).

**Interfaces — Consumes:** Task 1 (`Origin`, `Row`, `rows`, `step`, `default_row`, `Choice`, `start_label`,
`status_word`, `SHORTCUTS`, `Found`, `agent_detect::check`). **Produces:**
- `pub enum WelcomeEvent { Start { folder: PathBuf, choice: Choice }, Skip }`.
- `pub struct Welcome { .. }` (`Focusable`; `EventEmitter<WelcomeEvent>`): `pub fn new(origin: Origin, folder:
  PathBuf, presets: Vec<Preset>, keys: WelcomeKeys, window, cx) -> Self` where `pub struct WelcomeKeys { pub presets:
  Vec<Option<String>>, pub shortcuts: [String; 3] }` (from `keys_for(&binding_table(persistent), …)`);
  `pub fn set_found(&mut self, found: Vec<Found>, cx)` (applies `default_row` unless moved);
  `pub fn set_folder(&mut self, folder: PathBuf, cx)`; `pub fn origin(&self) -> Origin`; ui-shots:
  `pub fn chosen(&self) -> Row`, `pub fn folder(&self) -> &Path`, `pub fn found(&self) -> &[Found]`.
  The card snapshots the presets at open.
- `impl Workspace { pub(crate) fn open_welcome(&mut self, origin: Origin, window, cx);` (closes other overlays, sets
  `welcomed = true` + `save()`, default folder per note 62 §3, spawns `agent_detect::check` on the background
  executor unless `agents_frozen`, focuses the card) `fn on_welcome_event(..)` (Skip → `welcome = None`,
  `focus_active`; Start → Task 3's `run_welcome_pick`, a stub that closes in this task) `}`.
- Choose… is wired in Task 3; here the folder row and button render and Return on `Folder` emits nothing.

- [ ] **Step 1: failing tests**
  - `welcome_inks_meet_the_floors` (welcome.rs): the Global Constraints contrast list on every built-in theme.
  - `the_three_shortcuts_are_bound` (welcome.rs): with `binding_table(false)`, the `SHORTCUTS` actions' keys are
    `["⌘K", "⌘N", "⇧⌘U"]`, and `workspace::LaunchPreset1` → `"⌃⌘1"`.
  - ui-shots (Mocha, Latte), presets `claude`, `opencode`, `codex`, one shot terminal in `/Users/me`:
    - `welcome`: `open_welcome(FirstRun{over})`, `set_found([Yes, No, Yes])` → `chosen() == Preset(0)`; a click on
      the `codex` row → `chosen() == Preset(2)` and the label `Start codex`; the card is
      520 pt wide and its centre within 1 pt of the window's centre (below the band); its text includes `Welcome to
      fuxx`, `~`, `Not found on your PATH` once, `⌃⌘1`, `⌘K`, `⌘N`, `⇧⌘U`, `Start claude`; a pixel of the chosen row
      = `row_active`; a pixel of the Start button = `accent`; a scrim pixel left of the card is darker than the same
      pixel with no card; a top-band pixel is unchanged. With no presets: the text includes `No agent presets set.`
      and `chosen() == Plain`.
    - `welcome-checking`: no `set_found` → three `Checking…`, `chosen() == Preset(0)`; Enter emits `Start` with
      `Preset(claude)`; two frames 200 ms apart pixel-identical (Animations on and Reduce motion on).
      `set_found([No, No, No])` after ↓ → `chosen()` stays `Preset(1)` (moved); on a fresh card → `Plain`.
    - `welcome-keys`: ↓ ×4 from `Preset(0)` → `Folder` (wrap); ↑ → `Plain`, label `Open Terminal`; Esc → `welcome`
      is `None`, the terminal's handle focused; reopen, click the scrim → `None`; reopen, `open_palette(All)` →
      `welcome` `None`; reopen, blur the card (focus the terminal from code) → still open.
    - `welcome-narrow`: 480 × 420 window, nine presets, folder of 120 characters → the card's edges ≥ 16 pt inside
      the window and below 38 pt; `chosen()` moved to `Preset(8)` is inside the list's visible bounds; the folder
      text's right end visible (start cut).
    - update `overlays`: `Overlay::Welcome => ws.open_welcome(Origin::Help, window, cx)`; pages hidden as for the
      others.
- [ ] **Step 2: run** `cargo test` → FAIL; `__ui-shots welcome` → FAIL.
- [ ] **Step 3: implement** (`key_context("Welcome")`, `on_key_down` for `up`/`down`/`enter`/`escape` with no
  modifiers, `cx.stop_propagation()` on those; the agent list `overflow_y_scroll` + `track_scroll`; events from a card
  that isn't `self.welcome` ignored).
- [ ] **Step 4: run** `cargo test`, clippy (both), fmt, `__ui-shots welcome`, `__ui-shots overlays`, the full
  `__ui-shots`.
- [ ] **Step 5: commit** `feat(m11k): the Welcome card — folder, agents with detection, three shortcuts`.

### Task 3: The pick, the folder panel and the first run

**Files:** Modify `src/welcome.rs` (Choose… and Return on `Row::Folder` → the panel), `src/workspace.rs`
(`open_first`'s callback, `run_welcome_pick`), `src/workspace_daemon.rs` (only if its
`open_first` path needs nothing more — check, no change expected), `src/ui_shots.rs` (scenes below).

**Interfaces — Consumes:** Task 1 (`start_launch`, `pick_effect`, `Effect`, `welcome_due`), Task 2 (`Welcome`,
`WelcomeEvent`, `open_welcome`). **Produces:** `impl Workspace { pub(crate) fn run_welcome_pick(&mut self, folder:
PathBuf, choice: Choice, window, cx); fn over_alone(&self, over: u64) -> bool }`; `Welcome::choose_folder(&mut self,
window, cx)` (`cx.prompt_for_paths` with ⌘O's options; on `Some(path)` → `set_folder`; refocuses itself and restores
the agent row chosen before, if still open).

- [ ] **Step 1: failing tests** — ui-shots (Mocha, Latte):
  - `welcome-start` (a) first terminal `s1` in its own workspace, `open_welcome(FirstRun{over: s1})`,
    `set_folder("/w/app")`, choose `Preset(2)` (codex), Enter → `welcome` `None`; one session, not `s1`, its
    `project == Some("/w/app")`, `preset.name == "codex"`; one workspace. (b) as (a) but `s1`'s pane got a second tab
    first → two sessions remain, `s1` among them. (c) folder left at home, `Plain`, Enter → one session, `s1`.
    (d) `open_welcome(Help)` with two workspaces, pick `Preset(0)` → three workspaces, none closed.
  - `welcome-folder`: `choose_folder` answered by the scene's hook with `/w/other` → `folder() == "/w/other"`, the
    chosen row back to the agent row it was, the card focused; answered `None` → folder unchanged.
  - `welcome-first-run`: a workspace built with `welcomed = false` runs `open_first` → after its callback,
    `welcome.is_some()`, `origin() == FirstRun{over: first id}`, `welcomed == true`; with `welcomed = true` →
    `welcome` `None`.
- [ ] **Step 2: run** `__ui-shots welcome` → FAIL.
- [ ] **Step 3: implement** (Global Constraints run order and pick effect; `close_after` via `index_of` + `close_at`
  in `open_session_then`'s callback, only for `Some(new)`; the panel through a test hook in ui-shots, the real
  `prompt_for_paths` otherwise).
- [ ] **Step 4: run** `cargo test`, clippy (both), fmt, `__ui-shots welcome`, the full `__ui-shots`.
- [ ] **Step 5: commit** `feat(m11k): start an agent from the Welcome card; it shows on a first run`.

### Task 4: Help → Welcome to fuxx, Show Tips Card, the palette

**Files:** Modify `src/main.rs` (`ShowWelcome` in `actions!(help, …)`, `cx.on_action` → `with_help_window(..,
open_welcome(Origin::Help))`, the Help menu test), `src/help_menu.rs` (items), `src/palette_items.rs` (a `Welcome to
fuxx` command; `Show Welcome Tips` → `Show Tips Card` with keywords `welcome tips`), `src/ui_shots.rs` (palette scene
expectations that name the old label, if any).

**Interfaces — Consumes:** Task 2 (`open_welcome`, `Origin::Help`).

- [ ] **Step 1: failing tests**
  - the Help menu test: items exactly `Welcome to fuxx` (ShowWelcome), `—`, `Keyboard Shortcuts`, `fuxx Command
    Reference`, `—`, `Notifications Status…`, `Show Tips Card` (ShowWelcomeTips), `—`, `Report an Issue…`, `Show Logs`.
  - `every_bound_or_menu_action_is_in_the_palette_or_left_out` passes with `help::ShowWelcome` (fails until the
    command exists).
  - `welcome_is_found_in_the_palette` (palette_items): the `Welcome to fuxx` row matches `welcome` and `first run`;
    `Show Tips Card` matches `welcome tips`; group `WindowAndApp`, place Anywhere.
  - ui-shots `welcome-reopen` (Mocha, Latte): dispatch `ShowWelcome` → the card is open with `origin() == Help` and
    the folder = the active terminal's project.
- [ ] **Step 2: run** `cargo test`, `__ui-shots welcome-reopen` → FAIL.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** `cargo test`, clippy (both), fmt, the full `__ui-shots`, `scripts/bundle-mac.sh --app-only` (a
  release build).
- [ ] **Step 5: commit** `feat(m11k): Help → Welcome to fuxx; Show Tips Card`.

### Task 5: Merge and push (pre-authorized, autonomous program)

- [ ] Full gate on the branch: `cargo test`, `cargo clippy --all-targets` (both feature sets), `cargo fmt --check`,
  full `__ui-shots` green; a whole-branch review; then merge `m11k-first-run` into `master` and push.

### Task 6: Release — folded into the one release after M11l

- [ ] ~~Version bump, `chore(release): …`, tag, watch the Release run~~ Skipped by ruling: one release after M11l
  (your rule, 2026-11-08).
- [ ] You: the 15-item on-screen checklist in note 62 (after the combined release).

### Task 7: Docs

- [ ] Note 62: status, an "M11k as built" section (what the build settled or changed), the checklist's state. This
  plan (ticks, header). The roadmap (`40 - Roadmap.md`). The Plan Tracker. The Progress Log. Note 56 (Help menu: a
  line that "Show Welcome Tips" is now "Show Tips Card" and Welcome to fuxx leads the menu). `STRUCTURE.md`
  (`welcome.rs`, `welcome_items.rs`, `agent_detect.rs`). `README.md` (the Welcome card; Help → Welcome to fuxx; Show
  Tips Card).
- [ ] Commit `docs(m11k): …`.
