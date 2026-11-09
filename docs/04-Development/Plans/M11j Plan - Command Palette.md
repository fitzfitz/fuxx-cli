---
tags: [development, plan, milestone]
milestone: M11j
status: built
updated: 2026-11-09
---

# M11j Plan — Command palette (⌘K)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[61 - Milestone 11j - Command Palette]] (incl. "Settled before planning") |
| **Status** | 🟨 Built, reviewed, merged and pushed 2026-11-09 (1141 tests); release with M11l; on-screen check pending |
| **Tasks done** | 7 / 8 (Task 7, the release, is folded into the one release after M11l) |
| **▶ Pick up here** | Nothing to build. Your on-screen check in note 61 after the combined release |
| **Decisions used** | Yours (2026-11-08): a ⌘K palette for actions, workspaces, files and settings that replaces ⌘P. Everything else: AI's calls in note 61 (you skipped the design questions). |
| **Needs your eyes** | 👀 Task 7 (folded into the one release after M11l), then the 19-item on-screen check in note 61 |

**Goal:** ⌘K opens one search over actions, workspaces, files and settings; ⌘P opens it on Workspaces; the ⌘P
switcher panel goes.

**Architecture:** Pure cores first, GPUI second. `palette_match.rs` is the fuzzy matcher (Task 1);
`palette_items.rs` holds scopes, keys, Recent (Task 1), the action table, the settings rows and `compose`, which
turns the sources and the query into the list's lines (Task 2); `palette_files.rs` lists and ranks files off the UI
thread (Task 3). `palette.rs` is the overlay view and the `Workspace` glue that replaces the `Switcher` view (Task
4: workspaces and actions; Task 5: files, settings). Every pick closes the palette, gives the keyboard back to the
focused tab, then runs (`window.dispatch_action` for actions).

**Tech Stack:** GPUI at the pinned rev (`1d2a4b3f…`), only APIs already used here or checked in its source
(`Window::dispatch_action` window.rs:2045, `StyledText::with_highlights`, `HighlightStyle.font_weight`); git as a
process through `git_branch::run_git_bytes`. **No new crates** (so no Context7 look-up: no new crate API).

**Spec:** [[61 - Milestone 11j - Command Palette]]

## Global Constraints

- **No new crates** (`[dependencies]` unchanged), **no new settings**, no change to the scripting API, MCP or the
  CLI. Nothing writes a settings or session file that didn't before (Recent is in memory).
- **Nothing blocks the UI thread:** `find_root`, git, the walk and file ranking run on the background executor.
  Workspaces, actions and settings are ranked on the UI thread (they are few).
- **Keys:** `cmd-k` → `workspace::OpenPalette` (no context, in `key_bindings()` next to `cmd-p`); `cmd-p` →
  `workspace::OpenSwitcher` (kept; now opens the palette on Workspaces). In context `Palette`: `up` →
  `palette::SelectPrev`, `down` → `palette::SelectNext`, `tab` → `palette::NextScope`, `shift-tab` →
  `palette::PrevScope`. Enter, Esc, ⌘⌫, ⌘V are the `NameInput`'s.
- **Menus:** File menu: `Command Palette…` (OpenPalette) then `Go to Workspace…` (OpenSwitcher, was "Go to
  Session…") then `Go to What Needs You`. **Help list** (`describe`): `workspace::OpenPalette` →
  `Listed(WindowAndApp, "Command Palette…")`; `workspace::OpenSwitcher` → `Listed(Workspaces, "Go to Workspace…")`;
  prefix `palette::` → `Unlisted`; the `switcher::` arm goes.
- **Toggle rule** (`palette_step`): closed → Open; open on the asked scope → Close; open on another → Rescope (text
  kept). ⌘K asks All, ⌘P asks Workspaces. Sidebar row menu "Show All Workspaces" opens on Workspaces.
- **Scopes:** order `All, Actions, Workspaces, Files, Settings`; Tab next, ⇧Tab previous, both round. Chip labels
  exactly `All`, `Actions`, `Workspaces`, `Files`, `Settings`. Placeholders exactly: All `Search actions,
  workspaces, files and settings`; Actions `Run an action`; Workspaces `Go to a workspace \u{2014} number, name,
  group or question`; Files `Open a file`; Settings `Find a setting`. On All only, a query starting with `>`
  searches Actions (rest trimmed at its start), the Actions chip lit.
- **Matcher** (`palette_match`): case-insensitive per char; query split on whitespace; every word a subsequence of
  the main text, words independent. Per word, the best placement: `+16` per letter, `+24` at a word start (index 0,
  after one of ` /-_.:(`, or lower→upper), `+16` right after the previous matched letter, `−1` per skipped letter
  between matches (cap `−12` per gap), `−1` per letter before the first (cap `−8`). Item = sum of words, `+50`
  (`PREFIX`) when the text starts with the whole trimmed query, `+1000` (`EXACT`) when it equals it. Files: main
  text = the path from the root, `+8` (`NAME_BONUS`) per matched letter after the last `/`. Loose match (main text
  fails, every word a substring of main + rest, or the keys contain `key_label(query.replace(' ', "-"))`):
  score `1` (`LOOSE`), no highlights. Workspace number (`3`, `#3`) → `2000` (`NUMBER`). Order in a kind: score
  desc, then Recent (newer first), then the kind's order (workspaces `urgency_order`; actions and settings table
  order; files shorter path, then by path).
- **Lists** (`compose`): `SECTION_CAP = 5`; empty All = Recent (≤ `RECENT_SHOWN` 5), Workspaces (≤ 5), Actions
  (≤ 5), each capped section ending `More`; Recent items not repeated nor counted. Typing in All: no Recent;
  sections by best score (ties Workspaces, Actions, Files, Settings), each ≤ 5 + `More`. Narrowed: all matches
  (files ≤ `FILE_RESULTS` 200, then a note `… N more files`); empty: Recent of that kind (≤ 5), then everything:
  Workspaces by urgency; Actions under `ShortcutGroup` headings; Settings under section headings; Files a note.
  Files' notes (SSH, no folder, listing, failed, type to search) show only in the Files scope; on All, Files
  appears only once there are hits (the `capped` note then follows its rows).
- **Copy exactly:** headings `Recent`, `Workspaces`, `Actions`, `Files in {root name}`, `Settings` (narrowed empty:
  the `ShortcutGroup::label()`s and `General`, `Appearance`, `Sessions`, `Browser`); more `… {n} more workspaces` /
  `actions` / `files` / `settings` (n = 1: `workspace`, `action`, `file`, `setting`); notes `Listing files…`,
  `Files aren't searched for SSH terminals.`, `No terminal folder to search.`, `Files couldn't be listed.`, `Only the
  first 50,000 files are searched.`, `Type to search files in {root name}`; empty list `Nothing matches`; footer
  `Enter open \u{b7} \u{2191}\u{2193} choose \u{b7} Tab scope \u{b7} Esc close`.
- **Look:** `overlay_panel::shell(&ui, PANEL_WIDTH)` with `PANEL_WIDTH = 600.0`, placed by `top_panel`; filter box
  `overlay_panel::filter_box`; chip row 2 pt margin, chips `CHIP = 22.0` tall, 10 pt side padding, 4 pt apart,
  radius 5, `text::META`; current chip `row_active` + `text`, others no fill + `text_dim`, `panel_hover` on hover.
  List `max_h = clamp(window_h − 200, 120, LIST_MAX = 420)`. Rows: `switcher::ROW` 28 / `WAITING_ROW` 42, 8 pt side
  padding, radius 6, `ROW_GAP` 2; icon slot `MARK_SLOT` + 8 pt; name `text::NAME` `text`, truncated; right side
  `text::META`. Selected `row_active` + name `FontWeight::MEDIUM`; hover `panel_hover`. Matched letters
  `FontWeight::SEMIBOLD`, same colour. Workspace rows exactly the switcher's. Action: empty slot, keys right in
  `text_dim` tabular. File: `Glyph::File` 12 pt in `palette_file_ink(kind, theme)`, name, 6 pt, folder `text_faint`
  start-cut. Setting: `Glyph::Gear` `text_dim`, section right in `text_faint`. More: empty slot, `text_dim`.
  Headings as `help_panel::render_group`. Footer as the switcher's.
- **Contrast:** `palette_file_ink(kind, theme) = readable(kind_color(kind, theme), &[ui.panel, ui.panel_hover,
  ui.row_active], MARK_MIN)` with `ui = theme.ui()`; on every built-in theme `text`, `text_dim`, `text_faint` ≥ 4.5 on `panel`,
  `panel_hover`, `row_active`; every `FileKind` ink ≥ 3.0 there.
- **Files:** folder = `places.folder(followed)` else the session's `project`; SSH → note. Root by
  `right_sidebar::find_root`. Repo: `git ls-files -z --cached --others --exclude-standard` minus `git ls-files -z
  --deleted` (`run_git_bytes`, `GIT_CAP`); paths `file_tree::nfc`, deduped, sorted. Not a repo: breadth-first walk,
  `.git` and dot-folders skipped, symlinked folders not followed, `WALK_FILES = 20_000`, `WALK_DEPTH = 12`,
  `WALK_TIME = 2 s`. Kept ≤ `FILES_CAP = 50_000` (`more` = the rest). Re-list when missing or older than `FRESH =
  10 s`, one listing at a time per window. Ranking off-thread with a generation, checking the shared counter every
  `CANCEL_EVERY = 1_024` paths. Pick → `open_file_tab(root.join(rel), FileMode::Plain, ..)`.
- **Run order on a pick:** `self.palette = None`, `focus_active`, then: action → `window.dispatch_action(run(),
  cx)`; New Browser Tab → `new_browser_tab` on `model.focused_pane()`; workspace → as the switcher's Jump (`select`
  of `rollup_session`, else `select_workspace`); file → `open_file_tab`; setting → `preferences::open_at`. `More` →
  `set_scope(kind's scope)`, stays open. Every pick but More → `recent.push(key)`.
- Every visual change has ui-shots in **Catppuccin Mocha and Latte**. Existing tests keep passing; scenes whose
  pixels or flow change are updated in the task that changes them (named per task).
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo clippy --all-targets` (both feature sets),
  `cargo fmt --check`, `cargo run --features ui-shots -- __ui-shots [filter]`, `scripts/bundle-mac.sh --app-only`.

### The action table (`palette_items::COMMANDS`, in this order)

Place: A = Anywhere, T = terminal, F = file tab, B = browser tab, TF = terminal or file tab, P = Anywhere while
persistent sessions are on. Keywords in brackets.

| Group | Name — action — place |
|---|---|
| Workspaces | New Workspace — `NewWorkspace` — A · New Workspace in Folder… — `NewSessionInFolder` — A [open folder] · New SSH Workspace… — `NewSshSession` — A [ssh host remote] · Rename Workspace… — `RenameSession` — A · Move Workspace Up — `MoveSessionUp` — A · Move Workspace Down — `MoveSessionDown` — A · Next Workspace — `NextSession` — A · Previous Workspace — `PrevSession` — A · Go to What Needs You — `NextNeedingYou` — A [waiting done urgent] · Open Browser… — `OpenBrowser` — A [web url address]; then **Start {preset}** — `LaunchPreset{n+1}` — A [agent preset], one per preset n < 9 |
| Tabs | New Tab — `NewTab` — A · New Browser Tab — (`Run::NewBrowserTab`) — A [web] · Close Tab — `CloseSession` — A · Next Tab — `NextTab` — A · Previous Tab — `PrevTab` — A · Find in File… — `file_tab::Find` — F [search] · Find Next — `file_tab::FindNext` — F · Find Previous — `file_tab::FindPrev` — F |
| Panes | Split Right — `SplitRight` · Split Down — `SplitDown` · Previous Pane — `PrevPane` · Next Pane — `NextPane` · Focus Pane Left / Right / Up / Down — `FocusPane*` · Equalize Panes — `EqualizePanes` — all A |
| Terminal | Copy — `terminal::Copy` — TF · Paste — `terminal::Paste` — T · Scroll Up a Page — `ScrollPageUp` — T · Scroll Down a Page — `ScrollPageDown` — T |
| Browser pages | Go to Address Field — `browser_tab::FocusAddress` · Reload Page — `Reload` · Page Back — `GoBack` · Page Forward — `GoForward` · Zoom Page In — `ZoomIn` · Zoom Page Out — `ZoomOut` · Page Actual Size — `ResetZoom` — all B |
| Window & app | Toggle Sidebar — `ToggleSidebar` · Toggle Right Sidebar — `ToggleRightSidebar` · Bigger Text — `view::Bigger` [font size zoom] · Smaller Text — `view::Smaller` [font size zoom] · Actual Text Size — `view::ActualSize` [font size zoom] · Minimize — `window::Minimize` · Zoom Window — `window::Zoom` · Hide fuxx — `fuxx::Hide` · Preferences… — `fuxx::OpenPreferences` [settings] · Keyboard Shortcuts — `help::ShowShortcuts` [keys help] · fuxx Command Reference — `help::ShowCommands` [cli help] · Notifications Status… — `help::ShowNotificationStatus` · Show Welcome Tips — `help::ShowWelcomeTips` · Report an Issue… — `help::ReportIssue` [bug] · Show Logs — `help::ShowLogs` · Check for Updates… — `fuxx::CheckForUpdates` · Quit fuxx — `fuxx::Quit` · Quit fuxx and End All Sessions — `fuxx::QuitAndEndSessions` — P; the rest A |

`NOT_IN_PALETTE`: `workspace::SelectSession1`–`9`, `workspace::LaunchPreset1`–`9` (rows per preset instead),
`workspace::OpenSwitcher`, `workspace::OpenPalette`, `edit::Cut`, `edit::SelectAll`.

### The settings (`preferences::palette_settings()`, from `Section::ALL` × `controls()`, the Cursor heading skipped)

General: `Animations` [animations motion], `Check for updates` [check-updates], `Tips card` [tips welcome].
Appearance: `Theme` [theme colours dark light], `Font` [font-family], `Font size` [font-size], `Ligatures`
[font-feature], `Cursor style` [cursor-style bar block underline], `Cursor glide` [cursor-glide], `Cursor breathing
glow` [cursor-glow blink], `Cursor colour follows the agent` [cursor-color color], `Cursor fun` [cursor-fun trail
sparks]. Sessions: `Restore sessions` [session-restore], `Keep running after quit` [persistent-sessions]. Browser:
`Show address bar` [browser-address-bar], `Keep history and data` [browser-keep-data]. Switch keywords start with
`OnOff::key()`.

## Review Focus

1. **An action from the palette lands where its key would.** File tab's Find, a page's Reload, the terminal's Copy,
   app-level Preferences/Quit, and Rename Workspace (which opens another overlay) all run once, in the right place,
   and the palette never receives them. (Task 4: ui-shots `palette-run`.)
2. **Huge or odd folders.** A repo with 100k files or an unignored `node_modules`, a terminal in `~` (no repo), no
   git, a slow git, names with spaces / unicode (NFD) / a tab, deleted tracked files. Expected: capped, never a
   stall, a note when capped or failed, no deleted file listed. (Task 3: `list_repo_end_to_end`,
   `walk_stops_at_its_caps`, `rank_files_stops_when_a_newer_query_starts`.)
3. **Lists that move while open.** Statuses flipping (re-ranked workspaces), files arriving, a workspace or tab
   closing via a script. Expected: the selection stays on its item or falls to the first; Enter never acts on a
   stale row. (Task 2: `compose_keeps_keys_stable`; Task 4: `palette-refresh` scene.)
4. **Parity and keys.** Every binding and menu action is in the palette or deliberately out; Help lists ⌘K and ⌘P;
   ⌘K and ⌘P reach fuxx from a browser page and from a file tab. (Task 2: `every_bound_or_menu_action_is_in_the_palette_or_left_out`;
   Task 4: the browser key test with `cmd-k`.)
5. **Overlay hygiene.** One overlay at a time, pages hidden while open, focus back to the tab after Esc, a late
   "lost focus" from a replaced palette ignored, ⌘K/⌘P toggle and rescope, the + menu closed, still marks with
   Reduce motion. (Task 4: `overlays` with `Overlay::Palette`, `palette-keys`; Task 5: `palette-motion`.)

---

### Task 1: The matcher, scopes, keys and Recent (pure)

**Files:** Create `src/palette_match.rs`, `src/palette_items.rs`; Modify `src/main.rs` (`mod palette_match; mod
palette_items;`).

**Interfaces — Produces:**
- `palette_match.rs`: `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Match { pub score: i32, pub positions:
  Vec<usize> }` (char indices, sorted, unique); `pub const EXACT: i32 = 1000; PREFIX: i32 = 50; NAME_BONUS: i32 = 8;
  LOOSE: i32 = 1; NUMBER: i32 = 2000;` `pub fn fuzzy(query: &str, text: &str) -> Option<Match>` (blank query →
  `Some(score 0, no positions)`); `pub fn fuzzy_path(query: &str, rel: &str) -> Option<Match>`;
  `pub fn loose(query: &str, rest: &[&str]) -> bool`; `pub fn byte_ranges(text: &str, positions: &[usize]) ->
  Vec<std::ops::Range<usize>>` (adjacent positions merged).
- `palette_items.rs`: `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum Kind { Workspace, Action, File,
  Setting }`; `#[derive(Clone, Debug, PartialEq, Eq, Hash)] pub enum Key { Workspace(u64), Action(&'static str),
  Preset(usize), File(PathBuf), Setting(&'static str), More(Kind) }` with `pub fn kind(&self) -> Kind`;
  `#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)] pub enum Scope { #[default] All, Actions, Workspaces,
  Files, Settings }` with `pub const ALL: [Scope; 5]`, `label`, `placeholder`, `next`, `prev`, `pub fn kind(self) ->
  Option<Kind>`, `pub fn of(kind: Kind) -> Scope`; `pub fn parse_query(scope: Scope, text: &str) -> (Scope, &str)`;
  `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Step { Open, Close, Rescope }`;
  `pub fn palette_step(open: Option<Scope>, asked: Scope) -> Step`; `pub const RECENT_MAX: usize = 20;
  RECENT_SHOWN: usize = 5;` `#[derive(Default, Debug)] pub struct Recent { .. }` with `pub fn push(&mut self, key:
  Key)` (More ignored; dedupe to front; cap), `pub fn iter(&self) -> impl Iterator<Item = &Key>` (newest first),
  `pub fn rank(&self, key: &Key) -> Option<usize>` (0 = newest).

- [x] **Step 1: failing tests** (`palette_match`):
  - `fuzzy_finds_letters_in_order`: `fuzzy("sr", "Split Right")` positions `[0, 6]`; `fuzzy("SPLIT", "Split
    Right")` some; `fuzzy("zz", "Split Right")` None; `fuzzy("rs", "Split Right")` None (order matters).
  - `word_starts_and_runs_rank_higher`: ranking `["Toggle Right Sidebar", "Toggle Sidebar"]` by `fuzzy("tsb", _)`
    puts `Toggle Sidebar` first; `fuzzy("split", "Split Right").score > fuzzy("split", "Show Splits").score`.
  - `exact_and_prefix_bonuses`: `fuzzy("paste", "Paste").score − fuzzy("paste", "Paste All").score ≥ EXACT − 10`;
    `fuzzy("new", "New Tab").score > fuzzy("new", "Renew").score + PREFIX − 1`.
  - `words_match_independently`: `fuzzy("left pane", "Focus Pane Left")` and `fuzzy("pane left", …)` both some with
    the same positions set.
  - `paths_prefer_the_file_name`: ordering `["src/domain/mod.rs", "docs/maintenance.md", "src/main.rs"]` by
    `fuzzy_path("main", _)` → `src/main.rs`, `docs/maintenance.md`, `src/domain/mod.rs`; `fuzzy_path("ü", "new
    dir/ü.txt")` some.
  - `loose_matches_every_word_somewhere`: `loose("api auth", &["api", "edit auth.rs?"])` true; `loose("web", &["api"])`
    false.
  - `byte_ranges_follow_chars`: `byte_ranges("Split Right", &[0, 1, 6])` → `[0..2, 6..7]`; `byte_ranges("ü.txt",
    &[0])` → `[0..2]`.
  - (`palette_items`) `scopes_cycle_round`: `All.next() == Actions`, `Settings.next() == All`, `All.prev() ==
    Settings`; `parse_query(All, "> split")` → `(Actions, "split")`; `parse_query(Workspaces, ">x")` → `(Workspaces,
    ">x")`; `parse_query(All, "split")` → `(All, "split")`.
  - `palette_step_toggles_and_rescopes`: `(None, All)` Open; `(Some(All), All)` Close; `(Some(All), Workspaces)`
    Rescope; `(Some(Files), All)` Rescope.
  - `recent_keeps_the_last_twenty_newest_first`: push 25 distinct keys → 20, newest first; pushing an old key moves
    it to the front; `More(_)` isn't kept; `rank` of the newest is `Some(0)`.
- [x] **Step 2: run** `cargo test palette_` → FAIL (undefined).
- [x] **Step 3: implement.** `fuzzy` per word is a DP over (query letter i, text char j) keeping the best score
  ending with letter i at j (with and without a run), O(q·t); chars compared by `to_lowercase()` equality.
- [x] **Step 4: run** `cargo test`, `cargo clippy --all-targets` (both feature sets), `cargo fmt --check` → green.
- [x] **Step 5: commit** `feat(m11j): palette matcher, scopes, keys and Recent`.

### Task 2: The sources and the list (pure)

**Files:** Modify `src/palette_items.rs` (table, rows, `compose`), `src/help_shortcuts.rs` (`keys_for`, shared with
`shortcut_rows`), `src/main.rs` (`pub(crate) fn menu_action_names() -> Vec<String>`), `src/preferences.rs`
(`PaletteSetting`, `palette_settings`, `Control::palette_name`, `Control::keywords`, `Section::title` →
`pub(crate)`), `src/switcher.rs` (nothing removed yet; `Entry` and `urgency_order` consumed).

**Interfaces — Consumes:** Task 1; `switcher::{Entry, urgency_order}`; `help_shortcuts::{ShortcutGroup,
binding_pairs, key_label}`.
**Produces** (`palette_items.rs` unless noted):
- `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Place { Anywhere, Terminal, File, Browser, TerminalOrFile,
  Persistent }`; `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Focused { Terminal, Browser, File, Nothing }`;
  `pub fn applies(place: Place, focused: Focused, persistent: bool) -> bool`.
- `#[derive(Clone, Copy)] pub enum Run { Action(fn() -> Box<dyn gpui::Action>), NewBrowserTab }`;
  `pub struct Command { pub name: &'static str, pub group: ShortcutGroup, pub keywords: &'static str, pub place:
  Place, pub run: Run }`; `pub static COMMANDS: &[Command]` (the table, minus presets);
  `pub const NOT_IN_PALETTE: &[&str]`.
- `#[derive(Clone, Debug, PartialEq)] pub struct ActionRow { pub key: Key, pub name: String, pub group:
  ShortcutGroup, pub keys: Option<String>, pub rest: String }`; `pub fn action_rows(focused: Focused, persistent:
  bool, presets: &[String], pairs: &[(String, String)]) -> Vec<ActionRow>` (presets after `Open Browser…`, key
  `Preset(n)`, keys from `LaunchPreset{n+1}`).
- `help_shortcuts::keys_for(pairs: &[(String, String)], action: &str) -> Option<String>` (the same joined text a
  shortcut row shows).
- `preferences.rs`: `#[derive(Clone, Debug, PartialEq)] pub(crate) struct PaletteSetting { pub name: &'static str,
  pub section: Section, pub keywords: Vec<&'static str> }`; `pub(crate) fn palette_settings() -> Vec<PaletteSetting>`.
- `#[derive(Clone, Debug, PartialEq)] pub struct FileHit { pub rel: String, pub positions: Vec<usize>, pub score:
  i32 }`; `pub enum Files<'a> { Note(&'static str), Listing, Failed, Ready { root_name: &'a str, hits: &'a [FileHit],
  total: usize, capped: bool } }`.
- `#[derive(Clone, Debug, PartialEq)] pub enum Line { Header(String), Item { key: Key, positions: Vec<usize> },
  More { kind: Kind, count: usize }, Note(String) }`; `pub struct Sources<'a> { pub scope: Scope, pub query: &'a str,
  pub workspaces: &'a [Entry], pub actions: &'a [ActionRow], pub settings: &'a [PaletteSetting], pub files: Files<'a>,
  pub recent: &'a Recent, pub valid: &'a dyn Fn(&Key) -> bool }` (`scope`/`query` already through `parse_query`;
  workspaces in display order, so `#n` = place + 1); `pub fn compose(s: &Sources) -> Vec<Line>`;
  `pub fn selectable(lines: &[Line]) -> Vec<usize>`; `pub fn more_label(kind: Kind, n: usize) -> String`; the copy
  constants in Global Constraints (`NOTHING`, `LISTING`, `SSH_NOTE`, `NO_FOLDER`, `FAILED`, `CAPPED`, `HINT`).

- [x] **Step 1: failing tests:**
  - `every_bound_or_menu_action_is_in_the_palette_or_left_out`: for each `(action, _)` of `binding_pairs(key_bindings
    + new_bindings + persistent_bindings)` with `describe` `Listed`, and each name in `menu_action_names()`: it is
    `(c.run)()`'s `name()` for some `Action` command, or in `NOT_IN_PALETTE`; `COMMANDS` names unique.
  - `actions_apply_only_where_they_work`: `action_rows(Terminal, false, ..)` has `Copy`, `Paste`, no `Find in File…`,
    no `Reload Page`, no `Quit fuxx and End All Sessions`; `(File, ..)` has `Find in File…` and `Copy`, no `Paste`;
    `(Browser, true, ..)` has `Reload Page` and the persistent quit; `(Nothing, ..)` has neither Copy nor Find.
  - `action_rows_carry_keys_and_presets`: with the real pairs, `Split Down` keys `⇧⌘D`, `Scroll Up a Page` keys
    `⇧⇞ / ⌘↑`, `Move Workspace Up` none; presets `["Claude", "Codex"]` → `Start Claude` (`⌃⌘1`, key `Preset(0)`) and
    `Start Codex` right after `Open Browser…`.
  - `keys_for_matches_the_help_rows`: `keys_for(pairs, "view::Bigger") == Some("⌘= / ⌘+")`; for every row of
    `shortcut_list(false)` built from one action, `keys_for` gives its `keys`.
  - `palette_settings_cover_every_control` (in `preferences.rs`): 16 entries, names unique, in section order;
    contains `("Cursor breathing glow", Appearance)` with keyword `cursor-glow`; no entry named `Cursor`; every
    `OnOff` switch's `key()` is some entry's first keyword.
  - `empty_all_shows_recent_workspaces_and_actions`: 8 workspaces (the switcher fixture), recent = [Action("Split
    Right"), Workspace(id of #3)] → lines: `Header("Recent")`, those two items, `Header("Workspaces")`, 5 items in
    urgency order without #3, `More{Workspace, 2}`, `Header("Actions")`, 5 items without Split Right, `More{Action,
    n − 6}`; no Files or Settings lines.
  - `typing_orders_sections_by_their_best_match`: query `glow` → `Settings` section first with `Cursor breathing
    glow`; query `split` → `Actions` first with `Split Right`, `Split Down`; query `3` → workspace #3 first; with
    files `Ready` holding 7 hits for `main` → `Header("Files in fuxx-cli")`, 5 items, `More{File, 2}`;
    `compose` of a query nothing matches → `[]`; on All with `Files::Listing` or `Files::Note(SSH_NOTE)` no Files
    heading or note appears.
  - `narrowed_scopes_list_everything`: Workspaces + empty → all 8 in urgency order (the old ⌘P order); Actions +
    empty → headings `Workspaces`, `Tabs`, `Panes`, `Terminal`, `Browser pages`, `Window & app` with their rows;
    Settings + empty → `General`…`Browser`; Files + empty → `Note("Type to search files in fuxx-cli")`; Files + `x`
    with 250 hits (total 250) → 200 items then `Note("… 50 more files")`; `Files::Note(SSH_NOTE)` → that note;
    `Listing` → `LISTING`; `capped` → a `CAPPED` note after the items.
  - `compose_keeps_keys_stable`: the same sources with a workspace's status changed still yield the same key set;
    `selectable` skips headings and notes.
  - `loose_and_number_matches`: query `auth.rs` finds the waiting workspace by its question (no positions);
    `cmd d` finds `Split Right` by its keys; `#4` puts #4 first; `more_label(File, 1) == "… 1 more file"`.
- [x] **Step 2: run** `cargo test palette_items preferences help_shortcuts` → FAIL.
- [x] **Step 3: implement** as the signatures; `rest` = keywords + group label + keys; `valid` filters Recent only.
- [x] **Step 4: run** `cargo test`, clippy (both), fmt → green.
- [x] **Step 5: commit** `feat(m11j): palette sources — every action, setting and workspace, composed into a list`.

### Task 3: Files — listing and ranking off the UI thread

**Files:** Create `src/palette_files.rs`; Modify `src/main.rs` (`mod palette_files`), `src/workspace.rs`
(`pub(crate) palette_files: PaletteFiles` on `Workspace`, made in `blank`).

**Interfaces — Consumes:** `git_branch::{find_git, run_git_bytes}`, `right_sidebar::find_root`, `file_tree::nfc`,
`palette_match::fuzzy_path`, `palette_items::FileHit`.
**Produces:**
- `pub const FILES_CAP: usize = 50_000; WALK_FILES: usize = 20_000; WALK_DEPTH: usize = 12; WALK_TIME: Duration =
  2 s; FRESH: Duration = 10 s; FILE_RESULTS: usize = 200; CANCEL_EVERY: usize = 1_024;`
- `#[derive(Clone, Debug, Default, PartialEq, Eq)] pub struct Listed { pub paths: Vec<String>, pub more: usize }`
- `pub fn parse_z(bytes: &[u8]) -> Vec<String>` (NUL-split, empty skipped, lossy UTF-8, NFC)
- `pub fn list_repo(git: &Path, root: &Path) -> Option<Listed>` (blocking; None when either git call fails or hits
  the cap) = `list_repo_with(git, root, FILES_CAP)`; `pub(crate) fn list_repo_with(git: &Path, root: &Path, cap:
  usize) -> Option<Listed>`
- `pub fn walk(root: &Path, max_files: usize, max_depth: usize, max_time: Duration) -> Listed`
- `#[derive(Clone, Debug, PartialEq)] pub struct FilesAnswer { pub folder: PathBuf, pub root: PathBuf, pub listed:
  Option<Listed> }`; `pub fn list_files(folder: &Path) -> Option<FilesAnswer>` (blocking; None when `find_root` gave
  no answer → shown as Failed)
- `#[derive(Default)] pub struct PaletteFiles { .. }` with `pub fn due(&self, folder: &Path, now: Instant) -> bool`
  (none running, and missing or older than `FRESH`), `pub fn started(&mut self, folder: &Path)`,
  `pub fn finished(&mut self, folder: &Path, answer: Option<FilesAnswer>, now: Instant)`, `pub fn get(&self, folder:
  &Path) -> Option<&CachedFiles>` where `pub struct CachedFiles { pub root: PathBuf, pub paths: Arc<Vec<String>>, pub more:
  usize, pub failed: bool, pub listed_at: Instant }`, `pub fn running(&self) -> bool`.
- `pub fn rank_files(paths: &[String], query: &str, limit: usize, generation: u64, current: &AtomicU64) ->
  Option<(Vec<FileHit>, usize)>` (None when `current` moved past `generation` mid-way; else the best `limit` hits by
  the Global Constraints order and the total matched).
- `impl Workspace { pub(crate) fn palette_folder(&self) -> Option<(PathBuf, bool)>` (the followed terminal's
  folder and whether SSH) `; pub(crate) fn list_palette_files(&mut self, cx: &mut Context<Self>) }` (when `due`:
  `started`, then `list_files` on the background executor, then `finished` and `cx.notify()`).
- ui-shots only: `PaletteFiles::set_for_shots(&mut self, folder: PathBuf, root: PathBuf, paths: Vec<String>)`.

- [x] **Step 1: failing tests** (temp repos as `git_status`'s tests make them):
  - `parse_z_splits_and_normalises`: `b"a.rs\0dir/b c.txt\0\0"` → `["a.rs", "dir/b c.txt"]`; an NFD `u\u{308}.txt`
    → `"ü.txt"`.
  - `list_repo_end_to_end`: commit `a.rs`, `src/b.rs`, `gone.rs`; delete `gone.rs`; add untracked `new dir/ü.txt`;
    `.gitignore` with `target/` and a `target/x`; a file with a tab in its name → `paths == ["a.rs", "new dir/ü.txt",
    "src/b.rs", "tab\tname.txt"]` (sorted), `more == 0`; a plain folder → `None`.
  - `list_repo_caps`: 1,005 untracked files, `list_repo_with(git, root, 1_000)` → 1,000 paths, `more == 5`.
  - `walk_stops_at_its_caps`: a tree with `.hidden/x`, `.git/y`, a symlink to a folder, 30 files over 15 levels →
    no `.hidden` or `.git` path, nothing through the link; `walk(root, 10, 12, 2 s)` → 10 paths and `more ≥ 1`;
    depth 3 → no path with more than 3 `/`.
  - `palette_files_lists_once_and_freshens`: `due` true; after `started`, false (running); after `finished` at t,
    false at t + 9 s, true at t + 10 s; a failed answer → `get(..).failed`, and `due` true again at once.
  - `rank_files_orders_and_counts`: paths `["src/domain/mod.rs", "docs/maintenance.md", "src/main.rs", "README"]`,
    `main`, limit 2 → `src/main.rs`, `docs/maintenance.md`, total 3.
  - `rank_files_stops_when_a_newer_query_starts`: 10,000 paths, `current` set to `generation + 1` before the call →
    `None`.
- [x] **Step 2: run** `cargo test palette_files` → FAIL.
- [x] **Step 3: implement** (`list_repo`: the two `ls-files` calls, set difference, NFC, dedupe, sort, cap;
  `list_files`: `find_root`, then `list_repo` in a repo else `walk`).
- [x] **Step 4: run** `cargo test`, clippy (both), fmt → green.
- [x] **Step 5: commit** `feat(m11j): palette files — git or a bounded walk, ranked off the UI thread`.

### Task 4: The palette view — workspaces and actions, ⌘K, ⌘P

**Files:** Create `src/palette.rs`; Modify `src/switcher.rs` (remove `Switcher`, `SwitcherEvent`, `matches`,
`filter_rows`, its actions and render; keep `Entry`, `urgency_order`, `next_needing_you`, `switcher_entries`,
`jump_to_needing_you`, `ROW`, `WAITING_ROW` (now `pub(crate)`), `ROW_GAP`, `LIST_MAX_HEIGHT`; the workspace row
becomes `pub(crate) fn workspace_row(entry: &Entry, number: usize, chosen: bool, highlights: &[Range<usize>], phase:
f32, ui: &UiColors) -> Div`), `src/workspace.rs` (`Overlay::Palette` in `overlays!` in place of `Switcher`,
`palette: Option<Entity<Palette>>`, `palette_recent: Recent`, `close_overlays`, `OpenPalette` in `actions!` and
`on_action`s for it and `OpenSwitcher`, the render's `top_panel(palette)`), `src/pane_browser.rs`
(`Overlays.switcher` → `palette`), `src/sidebar.rs` (`palette_shown`), `src/tab_menu.rs` (ShowAll → Workspaces),
`src/main.rs` (`mod palette`, bindings, File menu, the browser key test adds `("cmd-k", Box::new(OpenPalette))`),
`src/help_shortcuts.rs` (`describe` per Global Constraints), `src/ui_shots.rs` (scenes below; `switcher` →
`palette-workspaces`; update `tabs-switcher`, `overlays`, `help`).

**Interfaces — Consumes:** Tasks 1–2 (`compose`, `action_rows`, `Scope`, `palette_step`, `Recent`, `byte_ranges`).
**Produces:**
- `actions!(palette, [SelectPrev, SelectNext, NextScope, PrevScope]);`
- `pub enum PaletteEvent { Pick(Key), Cancel }`; `pub struct Palette { .. }` (`Focusable` = its `NameInput`),
  `pub fn new(scope: Scope, sources: PaletteData, window, cx) -> Self` where `pub struct PaletteData { pub
  workspaces: Vec<Entry>, pub actions: Vec<ActionRow>, pub settings: Vec<PaletteSetting>, pub files: FilesData,
  pub phase: f32 }`,
  `pub fn refresh(&mut self, data: PaletteData)` (keeps the selected key), `pub fn scope(&self) -> Scope`,
  `pub fn set_scope(&mut self, scope: Scope, cx)`, `pub fn shown_workspaces(&self) -> impl Iterator<Item = &Entry>`;
  ui-shots: `pub fn lines(&self) -> &[Line]`, `pub fn selected_key(&self) -> Option<&Key>`.
- `impl Workspace { pub(crate) fn open_palette(&mut self, scope: Scope, window, cx);` (applies `palette_step`)
  `fn on_palette_event(..); pub(crate) fn refresh_palette(&self, cx); pub(crate) fn palette_shown(&self, cx: &App)
  -> Vec<(Status, Instant)>; fn focused_kind(&self) -> Focused; pub(crate) fn run_pick(&mut self, key: Key, window,
  cx) }` (Global Constraints run order).
- `pub(crate) fn palette_file_ink(kind: FileKind, theme: &Theme) -> Rgb` (drawn in Task 5; defined and tested here
  with the contrast test).
- `pub enum FilesData { Note(&'static str), Listing, Failed, Ready { root: PathBuf, paths: Arc<Vec<String>>, more:
  usize } }` (mapped to `palette_items::Files` for `compose`); in this task the Workspace always passes
  `FilesData::Note(NO_FOLDER)`, so the Files scope shows that note until Task 5.

- [x] **Step 1: failing tests:**
  - `command_k_and_p_are_listed` (help_shortcuts): `shortcut_list(false)` has `("Command Palette…", "⌘K")` under
    Window & app and `("Go to Workspace…", "⌘P")` under Workspaces; `describe("palette::NextScope") == Unlisted`.
  - `palette_inks_meet_the_floors` (palette.rs): every built-in theme, the Global Constraints contrast list.
  - the browser key test with `cmd-k` (fails until bound).
  - ui-shots (Mocha, Latte):
    - `palette`: the eight-workspace fixture (`switcher_workspace`), `open_palette(All)` → lines as
      `empty_all_shows_recent_workspaces_and_actions` with no Recent; the first row selected and filled
      `row_active`; the `All` chip's pixel = `row_active`; the panel is 600 pt wide.
    - `palette-workspaces`: `open_palette(Workspaces)` → shown names in the old `switcher` scene's order; typing
      `build` → the old `switcher-filter` result; Enter → that workspace is focused and the palette is `None`.
    - `palette-actions`: type `split r` → first item `Action("Split Right")`, its row shows `⌘D` text; Enter → the
      focused workspace has 2 panes and a terminal holds the focus; type `>` on a fresh palette → `scope() ==
      Actions` and the Actions chip pixel is `row_active`.
    - `palette-keys`: ↓ ×3 moves the selection over the `More` row and wraps; Tab → Actions, ⇧Tab ×2 → Settings;
      ⌘K on All → closed; ⌘K then ⌘P → open on Workspaces with the text kept; Esc → closed, the active terminal's
      handle focused; Enter on `More{Workspace, 3}` → scope Workspaces, still open.
    - `palette-run`: a workspace with a file tab focused: `find` lists `Find in File…`, Enter → the file tab's find
      bar is open; with a terminal focused `find` lists no Find; `Rename Workspace…` → the rename box is open and
      the palette is closed; `Preferences…` is dispatched once (count the app-level handler in the scene).
    - `palette-refresh`: open on Workspaces, select the third row, flip a lower workspace to Waiting → the
      selection keeps its workspace id; close that workspace's last tab → the selection is the first row.
    - `palette-recent`: pick Split Right, reopen → `Header("Recent")` then `Action("Split Right")`; pick workspace
      #2, reopen → Recent `[Workspace(#2), Action("Split Right")]`.
    - update `overlays` (opens `Overlay::Palette` via `open_palette(All)`), `tabs-switcher` (⌘P opens the palette),
      `help` (finds both new rows).
- [x] **Step 2: run** `cargo test` → FAIL; `__ui-shots palette` → FAIL.
- [x] **Step 3: implement:** the view rebuilds lines from `PaletteData` on each `refresh` and query change
  (`parse_query` then `compose`), keeps `selected: usize` into `selectable(lines)` and a `selected_key`; renders box,
  chips, list (`max_h` per Global Constraints, `track_scroll`, `scroll_to_item` on moves), footer; `key_context(
  "Palette")`, `occlude`, `on_mouse_down_out` → Cancel; picks emit `Pick(key)`, More handled in the view. The
  Workspace ignores events from a palette that isn't `self.palette` (as the switcher did).
- [x] **Step 4: run** `cargo test`, clippy (both), fmt, `__ui-shots palette`, `__ui-shots overlays`, `__ui-shots
  help`, the full `__ui-shots`.
- [x] **Step 5: commit** `feat(m11j): ⌘K command palette — actions and workspaces; ⌘P opens it on Workspaces`.

### Task 5: Files and settings in the palette

**Files:** Modify `src/palette.rs` (Files and Settings rows, the ranking task, `FilesData`), `src/preferences.rs`
(`pub(crate) fn open_at(section: Section, cx: &mut App)`), `src/workspace.rs` (`palette_data` fills files and
settings; `open_palette` calls `list_palette_files` when the scope or the query can show files), `src/ui_shots.rs`
(new scenes below).

**Interfaces — Consumes:** Task 3 (`PaletteFiles`, `rank_files`, `list_palette_files`, `palette_folder`,
`set_for_shots`), Task 4 (`Palette`, `run_pick`, `palette_file_ink`), Task 2 (`palette_settings`, `Files`).
**Produces:** in `Palette`: `generation: Arc<AtomicU64>`, `hits: Option<(u64, Vec<FileHit>, usize)>`,
`fn rank_files_for(&mut self, cx)` (spawns `rank_files` with `FILE_RESULTS` on the background executor when the
query or `paths` changed and the scope shows files; answers for an older generation dropped);
`preferences::open_at` (sets `LastSection`; an open window switches to it and is activated; else
`open_or_activate`).

- [x] **Step 1: failing tests:**
  - ui-shots (Mocha, Latte):
    - `palette-files`: `set_for_shots(folder, root "/w/fuxx-cli", ["src/main.rs", "src/view.rs", "docs/a.md", …])`,
      type `view` → `Header("Files in fuxx-cli")`, first item `File("/w/fuxx-cli/src/view.rs")`; the row has a
      pixel near `palette_file_ink(Red)` in the icon slot and `text_faint` pixels right of the name; Enter → the
      focused pane shows a file tab for that path in `FileMode::Plain`; Files scope, empty → the type-to-search
      note; an SSH terminal followed → `SSH_NOTE`; a folder listing running → `LISTING`.
    - `palette-settings`: type `glow` → first item `Setting("Cursor breathing glow")`, the row shows `Appearance`;
      Enter → the `LastSection` global is Appearance and the palette is closed (`open_at` has no unit test: it is
      one global write and the existing `open_or_activate`).
    - `palette-narrow`: a 480 pt wide window → the panel's right edge ≤ window − 16, a 60-letter file name row is
      cut, nothing past the panel.
    - `palette-motion`: Reduce motion on, a Working workspace in the list → two frames 200 ms apart pixel-identical
      in the panel.
    - `palette-recent` extended: pick a file and a setting → Recent shows them first, newest first.
- [x] **Step 2: run** → FAIL.
- [x] **Step 3: implement** (`palette_data` maps `palette_folder` + `palette_files.get` to `FilesData`; SSH →
  `Note(SSH_NOTE)`, none → `Note(NO_FOLDER)`, failed → `Failed`, running with nothing cached → `Listing`;
  `Ready` with `more > 0` → `capped`).
- [x] **Step 4: run** `cargo test`, clippy (both), fmt, `__ui-shots palette`, the full `__ui-shots`, and
  `scripts/bundle-mac.sh --app-only` (a release build).
- [x] **Step 5: commit** `feat(m11j): files and settings in the palette`.

### Task 6: Merge and push (pre-authorized, autonomous program)

- [x] Full gate on the branch: `cargo test`, `cargo clippy --all-targets` (both feature sets), `cargo fmt --check`,
  full `__ui-shots` green; a whole-branch review; then merge `m11j-command-palette` into `master` and push.

### Task 7: Release — folded into the one release after M11l

- [ ] ~~Version bump, `chore(release): …`, tag, watch the Release run~~ Skipped by ruling: one release after M11l
  (your rule, 2026-11-08).
- [ ] You: the 19-item on-screen checklist in note 61 (after the combined release).

### Task 8: Docs

- [x] Note 61: status, an "M11j as built" section (what the build settled or changed), the checklist's state. This
  plan (ticks, header). The roadmap (`40 - Roadmap.md`). The Plan Tracker. The Progress Log. `STRUCTURE.md`
  (`palette.rs`, `palette_items.rs`, `palette_match.rs`, `palette_files.rs`; `switcher.rs` now the workspace facts
  and ⇧⌘U). `README.md` (⌘K, ⌘P now the palette on Workspaces).
- [x] Commit `docs(m11j): …`.
