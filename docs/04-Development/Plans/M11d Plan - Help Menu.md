---
tags: [development, plan, milestone]
milestone: M11d
status: built
updated: 2026-11-08
---

# M11d Plan — Help menu

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[56 - Milestone 11d - Help Menu]] (the spec) |
| **Status** | 🟨 Built and reviewed 2026-11-08 (786 tests); on-screen check pending |
| **Tasks done** | 6 / 7 (Tasks 1–5, 7); Task 6 is your check |
| **▶ Pick up here** | Task 6: your on-screen check — 17 items in [[56 - Milestone 11d - Help Menu]], "On-screen checklist (pending)" |
| **Decisions used** | Yours (2026-11-06): a Help menu with searchable Shortcuts, plus all four extras. Yours (2026-11-07): "go", meaning skip the questions. Everything else is AI's call, set out in note 56. |
| **Needs your eyes** | 👀 Task 6 (on screen) |

**Goal:** A Help menu after Window: a searchable Keyboard Shortcuts panel (⌘/) and fuxx Command Reference built from
the real binding table and `fuxx help`'s text, a Notifications Status sheet, Show Welcome Tips, Report an Issue…, and
Show Logs (with the window's own `fuxx.log`).

**Architecture:** Two pure modules turn the sources into rows: `help_shortcuts.rs` reads `key_bindings()`, and
`help_commands.rs` parses `cli::USAGE`. One `HelpPanel` entity (`help_panel.rs`) shows either page over the window
and reuses `overlay_panel`, `NameInput` and the `Overlay` machinery, the way the ⌘P switcher does. Every Help action
is an app-level handler (`cx.on_action` in `main.rs`), so it works whatever window is key. The handlers that need a
window pick the main fuxx window by one rule (`help_target`). `help_menu.rs` holds the menu, the Help-menu marking
(objc2), the status copy, the issue URL and the window rule. `app_log.rs` redirects the bundled window's stderr to a
file.

**Tech Stack:** GPUI at the pinned rev (`window.prompt`, `cx.open_url`, `cx.open_with_system`), `objc2-app-kit` 0.3.2
(two more features), and the existing `native_notify`.

**Spec:** [[56 - Milestone 11d - Help Menu]]

## Global Constraints

- **No new crates.** `objc2-app-kit` gains exactly the features `"NSMenu"` and `"NSMenuItem"`. Nothing else in
  `Cargo.toml` changes.
- Menu order: `fuxx · File · Edit · Pane · View · Window · Help`. Help items, exactly: `Keyboard Shortcuts`,
  `fuxx Command Reference`, separator, `Notifications Status…`, `Show Welcome Tips`, separator, `Report an Issue…`,
  `Show Logs` (`…` is `\u{2026}`).
- `⌘/` (`"cmd-/"`, no context) opens Keyboard Shortcuts, and closes the panel when it is already open on that page.
  The Command Reference has no key.
- Panel width **560 px**; placeholders `Search shortcuts` / `Search commands`; empty results `No shortcut matches` /
  `No command matches`; hint line `Esc closes`.
- Shortcut groups, in order: `Workspaces`, `Tabs`, `Panes`, `Terminal`, `Browser pages`, `Window & app`. Command
  groups, in order: `Sessions`, `Driving a session`, `Sidebar and banners`, `Layout`, `Browser`, `fuxx itself`,
  `MCP setup`.
- The Notifications Status copy and buttons: exactly the table in note 56. Settings URL:
  `x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=io.github.fitzfitz.fuxx`.
- Issue URL base: `https://github.com/fitzfitz/fuxx-cli/issues/new?body=`. Body is exactly as in note 56, with the
  last line `fuxx {CARGO_PKG_VERSION} · macOS {os}`, or `macOS unknown` when `sw_vers` fails.
- Logs folder: `~/Library/Logs/fuxx/` (the parent of `daemon::log_path`). Window log: `fuxx.log`, cap **1 MiB**
  (1_048_576 bytes) checked at launch. First line of a run: `fuxx {version} started (pid {pid})`.
- Nothing blocks the UI thread: the permission read (≤ 2 s) and `sw_vers` run on the background executor.
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo run --features ui-shots -- __ui-shots [filter]`,
  and `scripts/bundle-mac.sh --app-only`, each with its own `CARGO_TARGET_DIR` in the session scratchpad.

## Review Focus

1. **A new binding added later with no name**, so the shortcut list silently misses it. Expected: a test fails.
   (Task 1: `every_binding_is_listed_or_marked_unlisted`.)
2. **⌘/ while the filter box has the keyboard**, or while another overlay (rename, ⌘P) is open. Expected: the panel
   toggles and the other overlay closes; nothing is typed into the box or the terminal. (Task 3: the `help` ui-shot
   (e)/(f); the `overlays` shot includes `Overlay::Help`.)
3. **A Help item with a popped-out browser window or Preferences as the key window, or with no fuxx window.**
   Expected: Report an Issue / Show Logs still work; the panel and sheet items go to the main fuxx window, and do
   nothing when there is none. No panic. (Task 3: `help_target` tests.)
4. **`cargo run` from a terminal.** Expected: stderr is *not* redirected, and Notifications Status says "Not running
   as fuxx.app". (Task 5: `should_redirect`; Task 4: `status_message(false, None)`.)
5. **Issue text with characters that break a URL** (`&`, `#`, `·`, newlines, spaces). Expected: percent-encoded so the
   whole body arrives. (Task 5: the `issue_url` round-trip test.)

---

### Task 1: The shortcut list (pure) — ✅

**Files:**
- Create: `src/help_shortcuts.rs`
- Modify: `src/main.rs` (`mod help_shortcuts;`, make `key_bindings` `pub(crate)`)

**Interfaces:**
- Produces: `pub enum ShortcutGroup { Workspaces, Tabs, Panes, Terminal, BrowserPages, WindowAndApp }` with
  `fn label(self) -> &'static str`; `pub struct ShortcutRow { pub group: ShortcutGroup, pub name: &'static str, pub keys: String }`;
  `pub enum Describe { Listed(ShortcutGroup, &'static str), Unlisted, Unknown }`;
  `pub fn describe(action: &str) -> Describe` (`action` is GPUI's `Action::name()`, e.g. `"workspace::SplitRight"`);
  `pub fn key_label(keystroke: &str) -> String`;
  `pub fn binding_pairs(bindings: &[KeyBinding]) -> Vec<(String, String)>` (action name, keystroke text);
  `pub fn shortcut_rows(pairs: &[(String, String)]) -> Vec<ShortcutRow>` (`Listed` only; grouped in enum order,
  first-seen order within a group, the same `(group, name)` merged);
  `pub fn row_matches(row: &ShortcutRow, query: &str) -> bool`.

- [x] **Step 1: failing tests**
  - `key_label`: `"cmd-shift-d"` → `"⇧⌘D"`; `"ctrl-cmd-="` → `"⌃⌘="`; `"cmd-alt-left"` → `"⌥⌘←"`; `"cmd-}"` →
    `"⇧⌘]"`; `"cmd-{"` → `"⇧⌘["`; `"shift-pageup"` → `"⇧⇞"`; `"ctrl-shift-tab"` → `"⌃⇧⇥"`; `"cmd-\\"` → `"⌘\\"`;
    `"cmd-/"` → `"⌘/"`. Modifier order is always ⌃⌥⇧⌘.
  - `shortcut_rows`: SelectSession1..9 → one row `Go to workspace 1–9`, keys `"⌘1 – ⌘9"`; Bigger with `cmd-=` and
    `cmd-+` → `"⌘= / ⌘+"`; Copy (`cmd-c`, Terminal) → group Terminal.
  - `row_matches`: "split" matches Split Right; "⌘d" and "cmd d" match Split Right (the query is compared with the
    label and with the raw keystroke, `-` read as a space); "" matches all; "zzz" none.
  - `every_binding_is_listed_or_marked_unlisted`: for every pair of `binding_pairs(&key_bindings())`, `describe` is
    not `Unknown`.
  - `groups_come_in_order`: the groups of `shortcut_rows(&binding_pairs(&key_bindings()))` are in enum order, none
    empty.
- [x] **Step 2: run** `cargo test --bin fuxx help_shortcuts` → FAIL (not defined).
- [x] **Step 3: implement.** `describe` is one `match` on the action name. `Unlisted`: every `name_input::*`,
  `host_picker::*`, `switcher::*`, `browser_tab::Suggest*`, `tab_menu::Dismiss`, `edit::Cut`, `edit::SelectAll`.
  Listed names: the menu titles where a menu item exists (`Split Right`, `Rename Workspace…`, `Go to What Needs You`
  …). The rest: `Go to workspace 1–9`, `Start agent preset 1–9`, `Next workspace` / `Previous workspace`,
  `Next tab` / `Previous tab`, `Scroll up a page` / `Scroll down a page`, `Copy` / `Paste`, `Address field` (⌘L), and
  in Browser pages `Reload` / `Back` / `Forward` / `Zoom in` / `Zoom out` / `Actual size`. Collapsing: ≥ 3 keys →
  `"{first} – {last}"`; 2 → `" / "`.
- [x] **Step 4: run** `cargo test`, `cargo clippy --all-targets` (and `--features ui-shots`), `cargo fmt --check` → ok.
- [x] **Step 5: commit** `feat(m11d): the shortcut list, built from the key-binding table`.

### Task 2: The command reference (pure) — ✅

**Files:**
- Create: `src/help_commands.rs`
- Modify: `src/cli.rs` (make `USAGE` and `COMMANDS` `pub(crate)`), `src/main.rs` (`mod`)

**Interfaces:**
- Produces: `pub struct CommandEntry { pub group: &'static str, pub usage: Vec<String>, pub description: String }`;
  `pub struct Reference { pub entries: Vec<CommandEntry>, pub footer: Vec<String> }`;
  `pub fn parse_usage(usage: &str) -> Reference`; `pub fn mcp_entry(exe: &Path) -> CommandEntry` (group `MCP setup`,
  usage `["claude mcp add fuxx -- {exe}"]`, description `Lets Claude Code drive fuxx (scripting needs fuxx running).`);
  `pub fn reference(exe: &Path) -> Reference` (`parse_usage(cli::USAGE)` + the MCP entry last);
  `pub fn entry_matches(entry: &CommandEntry, query: &str) -> bool` (usage, description or group, ignoring case).
- **Parse rules**, pinned by the tests: `DESC_COL = 39`. Skip the `usage:` line and blank lines. Indent 0 after the
  list → a footer line. Indent 2 starts an entry. On any line, if col 38 is a space and col 39 isn't → usage part
  `line[..39].trim()` (if non-empty) and description part `line[39..]`. Otherwise, a run of ≥ 3 spaces after
  non-space text → split there (the `progress` line). Otherwise: indent ≤ 6 → a usage line; indent 7 starting with
  `Every ` → a footer line; other indent-7 lines → added to the description. Description parts join with one space.
  The group comes from the usage's first word, per note 56.

- [x] **Step 1: failing tests** on `parse_usage(cli::USAGE)`:
  - 35 entries; the first is `ls` with description `list sessions (#N* focused, #N+ on screen)`.
  - The first `new`: usage `["new [--folder DIR] [--preset NAME] [--run CMD] [--name NAME]", "[--split right|down | --tab [--of TARGET]] [--background]"]`,
    description `open a session in a workspace of its own (--split: a pane beside TARGET's, --tab: a tab in it); prints its id`.
  - `progress`: usage `["progress [TARGET] 0-100 [--label TEXT] | --clear"]`, description `a progress bar under the row`.
  - `wait`: 2 usage lines; description starts `block until the condition holds (at once if it already does;`.
  - footer = `["Every browser command takes --session TARGET (default: your session).", "Every command but daemon takes --json. TARGET is s4, #2 or a session name.", "Exit codes: 0 ok, 1 refused or nothing to show, 2 bad arguments, 3 fuxx not reachable, 124 wait timed out."]`.
  - `every_command_has_an_entry`: for each name in `cli::COMMANDS`, some entry's first usage word is the name's first
    word, and each later word of the name is a word of that entry's usage (`browser fill` → `browser click REF | fill REF TEXT | press KEY`).
  - Groups in the Global Constraints' order; `mcp_entry(Path::new("/A/fuxx")).usage == ["claude mcp add fuxx -- /A/fuxx"]`.
  - `entry_matches`: "browser" matches every browser entry; "SNAPSHOT" matches `browser snapshot`; "zzz" none.
- [x] **Step 2: run** `cargo test --bin fuxx help_commands` → FAIL.
- [x] **Step 3: implement** per the parse rules.
- [x] **Step 4: run** `cargo test`, clippy (both), fmt → ok.
- [x] **Step 5: commit** `feat(m11d): the command reference, parsed from fuxx help's text`.

### Task 3: The Help panel, ⌘/ and the Help menu — ✅

**Files:**
- Create: `src/help_panel.rs`, `src/help_menu.rs`
- Modify: `src/workspace.rs` (field `help: Option<Entity<HelpPanel>>`; `Overlay::Help` in `overlays!` and its arm in
  `close_overlays`; `top_panel(help)` in render next to the switcher's), `src/main.rs` (`actions!(help, [ShowShortcuts,
  ShowCommands])`; `KeyBinding::new("cmd-/", ShowShortcuts, None)` and `up`/`down` → `help_panel::{ScrollUp, ScrollDown}`
  in context `"HelpPanel"`; app-level `cx.on_action` handlers; `help_menu::menu()` after `window_menu()` in
  `set_menus`; `mod`s), `src/help_shortcuts.rs` (`help::ShowShortcuts` → Listed `Window & app`, `Keyboard Shortcuts`;
  the two scroll actions → Unlisted), `src/ui_shots.rs` (new `("help", help_scenes)`; `Overlay::Help` arm in `overlays`)

**Interfaces:**
- Consumes: `binding_pairs`, `shortcut_rows`, `row_matches` (Task 1); `reference`, `entry_matches` (Task 2).
- Produces: `pub enum HelpPage { Shortcuts, Commands }`; `pub enum HelpPanelEvent { Close }`;
  `HelpPanel::new(page: HelpPage, window, cx)` (focuses its `NameInput`; key context `"HelpPanel"`);
  `pub fn page(&self) -> HelpPage`; `pub fn shown_rows(&self) -> usize` (for the shots);
  `Workspace::open_help(page: HelpPage, window, cx)`. When the panel is open on `page`, this closes it. When it is
  open on the other page, the new page replaces it. Otherwise it calls `close_overlays(Overlay::Help)` and opens.
  Closing returns focus with `focus_active`, as `on_switcher_event` does.
  `help_menu::menu() -> Menu`;
  `pub fn help_target(windows: &[(bool /*a main fuxx window*/, bool /*key*/)]) -> Option<usize>` (pure: the key
  window if it is a main fuxx window, else the first main fuxx window, else `None`). The handlers build the list from
  `cx.windows()` (main = `downcast::<Workspace>()` succeeds), then update that window's `Workspace`. Tasks 4–5 use
  the same rule.
- Panel layout: `overlay_panel::shell(&ui, 560.0)` → `filter_box` → grouped list (group label `text_faint` at
  `text::META`; rows at the switcher's `ROW_GAP`; max height = the switcher's `LIST_MAX_HEIGHT`; `overflow_y_scroll`)
  → the Commands page's footer lines → the hint line. Shortcut row: name left, keys right-aligned in `text_dim`.
  Command row: the usage lines in the terminal font family (`Appearance`'s), the description under them in
  `text_dim`. ↑/↓ scroll one row. A click outside, losing focus or `NameInput` Cancel → `Close`. Confirm does
  nothing.

- [x] **Step 1: failing tests**
  - `main.rs` `the_help_menu_comes_last_and_starts_with_the_panels`: the last menu passed to `set_menus` is named
    `"Help"`, and its items so far are `Keyboard Shortcuts` (`ShowShortcuts`) and `fuxx Command Reference`
    (`ShowCommands`). Test `help_menu::menu()` directly, the way `file_menu` is tested.
  - `help_target`: `[(false,true),(true,false)]` → `Some(1)`; `[(true,false),(true,true)]` → `Some(1)`;
    `[(false,true)]` → `None`; `[]` → `None`.
  - ui-shot `help`, in Catppuccin Mocha and one light theme: (a) the Shortcuts page, unfiltered; (b) "split" → exactly
    2 `shown_rows`; (c) "zzz" → `No shortcut matches`; (d) the Commands page filtered "browser"; (e) opened while ⌘P's
    switcher is open → `switcher` is `None` and the help box has focus; (f) `open_help(Shortcuts)` twice → closed,
    focus back on the active terminal.
  - `overlays` shot: an `Overlay::Help` arm (`ws.open_help(HelpPage::Shortcuts, window, cx)`).
- [x] **Step 2: run** `cargo test --bin fuxx help` and `cargo run --features ui-shots -- __ui-shots help` → FAIL.
- [x] **Step 3: implement.**
- [x] **Step 4: run** `cargo test`, clippy (both), fmt, the full `__ui-shots` → all ok. Look at the help PNGs: rows
  aligned and the keys right-aligned, in both themes.
- [x] **Step 5: commit** `feat(m11d): Help menu with Keyboard Shortcuts (⌘/) and the command reference`.

### Task 4: Notifications Status, Show Welcome Tips, and the macOS Help search — ✅

**Files:**
- Modify: `src/help_menu.rs`, `src/main.rs` (actions `ShowNotificationStatus`, `ShowWelcomeTips`, `ReportIssue`,
  `ShowLogs` in `help`'s `actions!`; handlers for the first two; after `cx.set_menus(..)`:
  `help_menu::mark_help_menu()`), `src/workspace.rs` or `src/sidebar.rs` (`show_welcome_tips(cx)`), `Cargo.toml`
  (`NSMenu`, `NSMenuItem`)

**Interfaces:**
- Consumes: `native_notify::{is_bundled, permission, Permission}`; `Workspace::show_hint`; `help_target` (Task 3).
- Produces: `pub struct StatusMessage { pub title: &'static str, pub detail: &'static str, pub buttons: &'static [&'static str], pub settings_button: Option<usize> }`;
  `pub fn status_message(bundled: bool, permission: Option<Permission>) -> StatusMessage` (pure; note 56's table;
  `settings_button` = index of `Open System Settings`; unbundled is always the "Not running as fuxx.app" row);
  `pub const NOTIFICATION_SETTINGS_URL: &str`; `pub fn mark_help_menu()` (main thread: finds the top-level item
  titled `Help` in `NSApplication::sharedApplication(mtm).mainMenu()`, calls `setHelpMenu` with its submenu, and does
  nothing if it isn't there); `Workspace::show_welcome_tips(cx)` (shows the sidebar if hidden, the state
  `ToggleSidebar` flips, then `show_hint`).
- `ShowNotificationStatus`: `cx.background_executor().spawn(|| (is_bundled(), permission()))` → on the target window,
  `window.prompt(PromptLevel::Info, title, Some(detail), buttons, cx)`; the settings button →
  `cx.open_url(NOTIFICATION_SETTINGS_URL)`.

- [x] **Step 1: failing tests**
  - `status_message` for (true, Some(Allowed)), (true, Some(Denied)), (true, Some(NotDetermined)), (false, None),
    (true, None), each checking the title, the buttons and `settings_button` (`None` only for (false, None)).
    (false, Some(Allowed)) gives the (false, None) message.
  - The help-menu test now lists all 8 items in the Global Constraints' order (6 actions, 2 separators).
  - ui-shot `help` (g): sidebar hidden and card dismissed → `show_welcome_tips` → sidebar shown,
    `hint_dismissed == false`.
- [x] **Step 2: run** → FAIL.
- [x] **Step 3: implement.** Report an Issue… and Show Logs are in the menu with their actions defined. Their
  handlers come in Task 5, and the items are greyed until then.
- [x] **Step 4: run** `cargo test`, clippy (both), fmt, `__ui-shots` → ok; `scripts/bundle-mac.sh --app-only` builds.
- [x] **Step 5: commit** `feat(m11d): Notifications Status, Show Welcome Tips, and the Help menu's search`.

### Task 5: Report an Issue…, Show Logs, and the window's log file — ✅

**Files:**
- Create: `src/app_log.rs`
- Modify: `src/help_menu.rs`, `src/main.rs` (GUI path, first thing: `app_log::start(&home)`; handlers for
  `ReportIssue`, `ShowLogs`; `mod`), `src/daemon.rs` (`pub fn logs_dir(home: &Path) -> PathBuf`, with `log_path`
  built on it)

**Interfaces:**
- Produces (help_menu): `pub fn issue_body(version: &str, os: Option<&str>) -> String`;
  `pub fn issue_url(version: &str, os: Option<&str>) -> String` (base + body with everything but `A–Z a–z 0–9 - . _ ~`
  encoded as UTF-8 `%XX`).
- Produces (app_log): `pub const LOG_CAP: u64 = 1_048_576`; `pub fn should_redirect(bundled: bool, stderr_is_tty: bool) -> bool`;
  `pub enum OpenMode { Append, Fresh }`; `pub fn open_mode(existing_len: Option<u64>) -> OpenMode`;
  `pub fn open_log(path: &Path) -> io::Result<File>` (per `open_mode`, and writes the first line);
  `pub fn start(home: &Path)` (if `should_redirect(native_notify::is_bundled(), isatty(2))`: creates `logs_dir`,
  `open_log`, `dup2`s it onto fds 1 and 2; any failure leaves stderr alone, silently). Declare `isatty` and `dup2`
  `extern "C"`, as `daemon_client.rs` declares `posix_spawn_*`.
- `ReportIssue`: `sw_vers -productVersion` on the background executor (trimmed; failure or empty → `None`) →
  `cx.open_url(&issue_url(env!("CARGO_PKG_VERSION"), os))`. `ShowLogs`: `create_dir_all(logs_dir(home))` →
  `cx.open_with_system(&dir)`. Neither needs a window.

- [x] **Step 1: failing tests**
  - `issue_body("0.4.0", Some("26.5"))` ends with `"---\nfuxx 0.4.0 · macOS 26.5"`; `None` gives `macOS unknown`.
  - `issue_url` starts with the base and has no raw space, `\n`, `&`, `#` or `·`. Percent-decoding what follows
    `body=` gives back `issue_body` exactly.
  - `should_redirect`: only `(true, false)` is true. `open_mode`: `None` → Append; `Some(LOG_CAP)` → Append;
    `Some(LOG_CAP + 1)` → Fresh.
  - `open_log` in a temp dir: a new file holds exactly the first line. An existing small file keeps its contents and
    gains the line. One over the cap holds only the line.
- [x] **Step 2: run** → FAIL.
- [x] **Step 3: implement.**
- [x] **Step 4: run** `cargo test`, clippy (both), fmt, `__ui-shots`, `scripts/bundle-mac.sh --app-only` → ok.
- [x] **Step 5: commit** `feat(m11d): Report an Issue, Show Logs, and the window's fuxx.log`.

### Task 6: On screen — 👀 pending (you)

The 17 items in [[56 - Milestone 11d - Help Menu]] → "On-screen checklist", on `target/dist/fuxx.app`. A failed
item goes back into the task that owns it.

### Task 7: Docs — ✅

- [x] Note 56: status, and any deviation from the plan recorded where it applies. This plan: ticks, status, tasks
  done. `40 - Roadmap.md`: the M11d line. The Plan Tracker: Where we are, plus the M11d row. `60 - Progress Log.md`:
  a new entry, with the status snapshot corrected. `STRUCTURE.md` (repo root): the 5 new modules.
- [x] Commit `docs(m11d): …`.
