---
tags: [development, plan, milestone]
milestone: M7
status: in-progress
updated: 2026-10-27
---

# M7 Plan — Session Ergonomics (groups, rename, reorder, presets, restore)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[47 - Milestone 7 - Session Ergonomics]] (the spec) |
| **Status** | 🟨 In progress |
| **Tasks done** | 0 / 8 |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | **Yours (2026-10-27):** build before the M6b check; presets = 3 built-ins + your own; restore = layout + re-run presets, as a setting. The rest are the AI's calls listed in the spec. |
| **Needs your eyes** | 👀 Task 7, done together with the M6b and M8 checks |

**Goal:** Project groups, rename, reorder, agent presets and session restore, as the spec describes.

**Architecture:**

```
 pure, unit-tested                          GPUI wiring (build + 👀)
 ───────────────────────────────            ─────────────────────────────────────────
 groups.rs   names, group labels,           workspace.rs  Session{name, project, preset}
             insert index, rows,                          sidebar rows/headers, drag/drop,
             rolled-up ring, same_group                   rename box, actions, save()
 session_list.rs  insert, move_item         name_input.rs NameInput (EntityInputHandler)
 presets.rs  Preset, builtins,              terminal_view.rs spawn(cwd, startup command)
             from_entries(agent-preset)     main.rs       menus, bindings
 restore.rs  RestoreMode, SavedState,       preferences.rs "Restore sessions" row
             encode/decode, launches        appearance.rs carries presets + restore mode
```

**Tech Stack:** GPUI at the pinned rev: `cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt })`
(returns a oneshot receiver of `Result<Option<Vec<PathBuf>>>`), `MenuItem::submenu(Menu)`, `ClickEvent::click_count()`,
`.on_drag(value, |value, offset, window, cx| preview_entity)` / `.on_drop(listener)` / `.drag_over::<T>(style)`,
`EntityInputHandler` + `ElementInputHandler` (see `crates/gpui/examples/input.rs` at the pinned rev).
**New crates:** `serde` (feature `derive`) and `serde_json`, at the versions already in `Cargo.lock` through GPUI
(serde 1.0.228, serde_json 1.0.150; checked via Context7 2026-10-27), so nothing new is downloaded.

**Spec:** [[47 - Milestone 7 - Session Ergonomics]] · [[12 - Scope]] · [[46b - Milestone 6b - Settings Window]]

## Global Constraints

- **Inherited:** everything from the M0–M6b plans. The only new crates are `serde` (derive) and `serde_json`, as above.
- **Gates:** 0 warnings from our code; `cargo fmt --check` clean; every test passes. Start: 118 tests.
- **Behaviour:** exactly the spec's "How it behaves", and nothing under "Not in M7".
- **Names:** user name > program title > preset name > `🖕` (via `groups::auto_name`), everywhere a session name shows.
- **Group invariant:** sessions without a project come first; each project's sessions are contiguous. Every insert uses `groups::insert_index`; every move is gated by `groups::same_group`.
- **Settings keys:** `agent-preset = Name: command` (repeatable) and `session-restore = agents | layout | off` (default `agents`). Read at startup through `appearance::load`, like the rest.
- **State file:** `~/Library/Application Support/fuxx/sessions.json`, written with `settings::write_atomic`. JSON as in Task 3; `version` = 1.
- **Failures never crash:** a bad state file, missing folder, failed write or failed spawn logs one `[fuxx] restore: …` / `[fuxx] presets: …` / `[fuxx] session: …` line and carries on.
- **No `cargo run` by subagents.** GUI behaviour is verified in Task 7 by the user.

## Review Focus

1. **A corrupt, partial or old state file** stops fuxx starting or loses sessions silently → `decode_rejects_garbage_and_other_versions`, `decode_fills_missing_fields` (Task 3); the write is atomic.
2. **Indices shift under a rename box or a drag** (a session closes or a new one opens meanwhile), so the wrong session is renamed or moved → the rename box is keyed by the session view's `EntityId`, not its index (Task 5); drops re-check `same_group` at drop time.
3. **A group gets split in two** by a move, drag or insert → `insert_index_keeps_groups_together`, `same_group_rules` (Task 1).
4. **A preset command with spaces, quotes or `--flags`** is mangled → `from_entries` keeps everything after the first `:` verbatim (trimmed) (`user_presets_add_replace_and_remove`, Task 2); it's typed into the shell as-is, followed by `\r`.
5. **Restore re-runs agents when set to `layout`**, or restores when `off` → `launches_follow_the_mode` (Task 3).

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `src/groups.rs` | Create | `auto_name`, `group_label`, `insert_index`, `rolled_up`, `Row`, `rows`, `same_group` | 1 |
| `src/session_list.rs` | Modify | `insert`, `move_item` | 1 |
| `Cargo.toml` | Modify | `serde` (derive), `serde_json` | 2 |
| `src/presets.rs` | Create | `Preset`, `MAX_PRESETS`, `builtins`, `from_entries` | 2 |
| `src/restore.rs` | Create | `RestoreMode`, `SavedSession`, `SavedState`, `encode`, `decode`, `state_path`, `Launch`, `launches` | 3 |
| `src/appearance.rs` | Modify | `Appearance` gains `presets: Vec<Preset>` and `restore: RestoreMode` | 3 |
| `src/terminal_view.rs` | Modify | `spawn(window, cx, cwd, startup)` | 4 |
| `src/workspace.rs` | Modify | Session fields, `open_session(Launch)`, new actions, sidebar groups, rename, drag, save/restore | 4–6 |
| `src/main.rs` | Modify | Menus and bindings | 4–5 |
| `src/name_input.rs` | Create | `NameInput` text box | 5 |
| `src/preferences.rs`, `src/prefs.rs` | Modify | "Restore sessions" row | 6 |

---

### Task 1: Groups and ordering, without GPUI — ⬜

**Files:** Create `src/groups.rs` · Modify `src/session_list.rs`, `src/main.rs` (`mod groups;`).

**Interfaces (produces):**
- `pub const DEFAULT_NAME: &str = "🖕";`
- `pub fn auto_name(user: Option<&str>, title: Option<&str>, preset: Option<&str>) -> String`: first of these that is `Some` and not blank after trimming (returned trimmed), else `DEFAULT_NAME`.
- `pub fn group_label(project: &Path, all: &[&Path]) -> String`: the folder's last component; if another **different** path in `all` has the same last component, append ` (<parent's last component>)`. A path with no last component (`/`) shows its full display string.
- `pub fn insert_index(projects: &[Option<&Path>], project: Option<&Path>) -> usize`: `None` → right after the last `None` in the leading run of `None`s (i.e. the count of leading `None`s); `Some(p)` → right after the last index equal to `p`, else `projects.len()`.
- `pub fn rolled_up(statuses: impl IntoIterator<Item = Status>) -> Status`: Waiting > Done > Working > Idle; empty → Idle.
- `#[derive(Clone, Debug, PartialEq, Eq)] pub enum Row { Header { project: PathBuf, first: usize, len: usize, collapsed: bool }, Session { index: usize, grouped: bool } }`
- `pub fn rows(projects: &[Option<&Path>], collapsed: &[PathBuf]) -> Vec<Row>`: one `Session { grouped: false }` per leading `None`; then for each project run a `Header`, followed by its `Session { grouped: true }` rows unless collapsed.
- `pub fn same_group(projects: &[Option<&Path>], a: usize, b: usize) -> bool`: both in range and `projects[a] == projects[b]`.
- `SessionList::insert(&mut self, index: usize, item: T) -> usize`: clamps `index` to `len`, inserts, makes it active, returns the index.
- `SessionList::move_item(&mut self, from: usize, to: usize) -> bool`: false (no change) if either is out of range; otherwise removes at `from` and inserts at `to`; the **same item** stays active.
- `SessionList::items_mut(&mut self) -> &mut [T]` if the workspace needs it.

- [ ] **Step 1: Failing tests** (in `groups.rs`, plus two in `session_list.rs`):

```rust
#[test]
fn auto_name_prefers_user_then_title_then_preset() {
    assert_eq!(auto_name(Some("api"), Some("vim"), Some("claude")), "api");
    assert_eq!(auto_name(Some("  "), Some(" vim "), Some("claude")), "vim");
    assert_eq!(auto_name(None, None, Some("claude")), "claude");
    assert_eq!(auto_name(None, None, None), "🖕");
}

#[test]
fn group_label_adds_the_parent_only_on_a_clash() {
    let (a, b, c) = (Path::new("/w/work/api"), Path::new("/w/home/api"), Path::new("/w/web"));
    assert_eq!(group_label(a, &[a, b, c]), "api (work)");
    assert_eq!(group_label(c, &[a, b, c]), "web");
    assert_eq!(group_label(a, &[a, a]), "api");
}

#[test]
fn insert_index_keeps_groups_together() {
    let (a, b) = (Path::new("/a"), Path::new("/b"));
    let p = [None, Some(a), Some(a), Some(b)];
    assert_eq!(insert_index(&p, None), 1);
    assert_eq!(insert_index(&p, Some(a)), 3);
    assert_eq!(insert_index(&p, Some(b)), 4);
    assert_eq!(insert_index(&p, Some(Path::new("/c"))), 4);
    assert_eq!(insert_index(&[], None), 0);
}

#[test]
fn rolled_up_picks_the_most_urgent() {
    assert_eq!(rolled_up([Status::Working, Status::Done, Status::Idle]), Status::Done);
    assert_eq!(rolled_up([Status::Done, Status::Waiting]), Status::Waiting);
    assert_eq!(rolled_up([Status::Idle, Status::Working]), Status::Working);
    assert_eq!(rolled_up([]), Status::Idle);
}

#[test]
fn rows_hide_collapsed_groups() {
    let (a, b) = (Path::new("/a"), Path::new("/b"));
    let p = [None, Some(a), Some(a), Some(b)];
    assert_eq!(
        rows(&p, &[a.to_path_buf()]),
        vec![
            Row::Session { index: 0, grouped: false },
            Row::Header { project: a.into(), first: 1, len: 2, collapsed: true },
            Row::Header { project: b.into(), first: 3, len: 1, collapsed: false },
            Row::Session { index: 3, grouped: true },
        ]
    );
}

#[test]
fn same_group_rules() {
    let (a, b) = (Path::new("/a"), Path::new("/b"));
    let p = [None, None, Some(a), Some(a), Some(b)];
    assert!(same_group(&p, 0, 1));
    assert!(same_group(&p, 2, 3));
    assert!(!same_group(&p, 1, 2));
    assert!(!same_group(&p, 3, 4));
    assert!(!same_group(&p, 3, 9));
}

// session_list.rs
#[test]
fn insert_puts_the_item_at_the_index_and_activates_it() {
    let mut l = list(3);
    assert_eq!(l.insert(1, 9), 1);
    assert_eq!(l.items(), &[0, 9, 1, 2]);
    assert_eq!(l.active(), Some(&9));
    assert_eq!(l.insert(99, 7), 4); // clamped to the end
}

#[test]
fn move_item_keeps_the_same_item_active() {
    let mut l = list(4);
    l.select(1);
    assert!(l.move_item(1, 3));
    assert_eq!(l.items(), &[0, 2, 3, 1]);
    assert_eq!(l.active(), Some(&1));
    l.select(0);
    assert!(l.move_item(3, 1));
    assert_eq!(l.items(), &[0, 1, 2, 3]);
    assert_eq!(l.active(), Some(&0));
    assert!(!l.move_item(0, 4));
}
```

- [ ] **Step 2: Run** `cargo test groups session_list`. Expected: compile errors (missing functions).
- [ ] **Step 3: Implement** as specified. `groups.rs` stays GPUI-free. Dead code is allowed until Tasks 4–5 (add `#[allow(dead_code)]` on the module items only if a warning appears, and remove it when they're used).
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (126), 0 warnings.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m7): pure group, name and ordering rules"`

### Task 2: Agent presets from the settings file — ⬜

**Files:** Modify `Cargo.toml` · Create `src/presets.rs` · Modify `src/main.rs` (`mod presets;`).

**Interfaces (produces):**
- `Cargo.toml`: `serde = { version = "1.0.228", features = ["derive"] }`, `serde_json = "1.0.150"` (with a one-line comment each, as the other deps have).
- `#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct Preset { pub name: String, pub command: String }`
- `pub const MAX_PRESETS: usize = 9;`
- `pub fn builtins() -> Vec<Preset>`: `claude`, `opencode`, `codex`, each with command = name.
- `pub fn from_entries(entries: &[Entry]) -> (Vec<Preset>, Vec<String>)` (presets, warnings). Starts from `builtins()`; walks entries with key `agent-preset` in order:
  - empty value → reset to `builtins()`;
  - no `:` or blank name → warning `agent-preset "<value>": expected "name: command"`, skipped;
  - `name:` with a blank command → remove that name (exact match, trimmed);
  - otherwise → replace the preset with that name in place, or append.
  - After the walk, keep the first `MAX_PRESETS`; if more, one warning `agent-preset: only 9 presets fit, skipped <names>`.
  - The command is everything after the **first** `:`, trimmed, kept verbatim.

- [ ] **Step 1: Failing tests:**

```rust
fn e(value: &str) -> Entry {
    Entry { key: "agent-preset".into(), value: value.into() }
}
fn names(p: &[Preset]) -> Vec<&str> {
    p.iter().map(|p| p.name.as_str()).collect()
}

#[test]
fn builtins_are_claude_opencode_codex() {
    let b = builtins();
    assert_eq!(names(&b), ["claude", "opencode", "codex"]);
    assert!(b.iter().all(|p| p.command == p.name));
}

#[test]
fn user_presets_add_replace_and_remove() {
    let (p, warnings) = from_entries(&[
        e("aider: aider --model sonnet"),
        e("claude: claude --continue \"fix: it\""),
        e("codex:"),
    ]);
    assert_eq!(names(&p), ["claude", "opencode", "aider"]);
    assert_eq!(p[0].command, "claude --continue \"fix: it\"");
    assert_eq!(p[2].command, "aider --model sonnet");
    assert!(warnings.is_empty());
}

#[test]
fn an_empty_value_resets_to_the_builtins() {
    assert_eq!(from_entries(&[e("codex:"), e("")]).0, builtins());
}

#[test]
fn bad_lines_and_overflow_are_skipped_with_warnings() {
    let mut entries = vec![e("nocolon"), e(": cmd")];
    for i in 0..7 {
        entries.push(e(&format!("p{i}: run{i}")));
    }
    let (p, warnings) = from_entries(&entries);
    assert_eq!(p.len(), 9);
    assert_eq!(p[8].name, "p5");
    assert_eq!(warnings.len(), 3);
}

#[test]
fn other_keys_are_ignored() {
    let other = Entry { key: "theme".into(), value: "x: y".into() };
    assert_eq!(from_entries(&[other]).0, builtins());
}
```

- [ ] **Step 2: Run** `cargo test presets`. Expected: compile errors.
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (131), 0 warnings.
- [ ] **Step 5: Commit:** `git add Cargo.toml Cargo.lock src && git commit -m "feat(m7): agent presets (built-in three + agent-preset lines)"`

### Task 3: The restore setting and the state file — ⬜

**Files:** Create `src/restore.rs` · Modify `src/appearance.rs`, `src/main.rs` (`mod restore;`).

**Interfaces (produces):**
- `#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)] pub enum RestoreMode { Off, Layout, #[default] Agents }`
  - `pub fn parse(s: &str) -> Option<Self>`: `off` / `layout` / `agents`, trimmed, any case.
  - `pub fn as_str(self) -> &'static str`: `"off"` / `"layout"` / `"agents"`.
  - `pub fn from_entries(entries: &[Entry]) -> (Self, Vec<String>)`: entries with key `session-restore` in order; empty value → default; invalid → warning `session-restore "<v>": expected agents, layout or off` (the previous value stays).
- `pub const VERSION: u32 = 1;`
- `#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)] pub struct SavedSession { #[serde(default)] pub name: Option<String>, #[serde(default)] pub project: Option<PathBuf>, #[serde(default)] pub preset: Option<Preset> }`
- `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct SavedState { pub version: u32, #[serde(default)] pub active: Option<usize>, #[serde(default)] pub collapsed: Vec<PathBuf>, #[serde(default)] pub sessions: Vec<SavedSession> }`
- `pub fn encode(state: &SavedState) -> String` (pretty JSON + trailing newline) · `pub fn decode(text: &str) -> Option<SavedState>`: `None` on bad JSON or `version != VERSION`.
- `pub fn state_path(home: &Path) -> PathBuf`: `home/Library/Application Support/fuxx/sessions.json`.
- `#[derive(Clone, Debug, PartialEq)] pub struct Launch { pub saved: SavedSession, pub run_preset: bool }` — also what the workspace uses to open any session.
- `pub fn launches(state: &SavedState, mode: RestoreMode) -> Vec<Launch>`: `Off` → empty; `Layout` → every session, `run_preset: false`; `Agents` → every session, `run_preset` = it has a preset.
- `Appearance` gains `pub presets: Vec<Preset>` and `pub restore: RestoreMode`, built in `load` from the same entries; each warning is logged as `[fuxx] config: <warning>`.

- [ ] **Step 1: Failing tests:**

```rust
fn r(value: &str) -> Entry {
    Entry { key: "session-restore".into(), value: value.into() }
}
fn sample() -> SavedState {
    SavedState {
        version: VERSION,
        active: Some(1),
        collapsed: vec![PathBuf::from("/w/api")],
        sessions: vec![
            SavedSession::default(),
            SavedSession {
                name: Some("🖕 api".into()),
                project: Some(PathBuf::from("/w/api")),
                preset: Some(Preset { name: "claude".into(), command: "claude".into() }),
            },
        ],
    }
}

#[test]
fn restore_mode_parses_and_defaults_to_agents() {
    assert_eq!(RestoreMode::parse(" Layout "), Some(RestoreMode::Layout));
    assert_eq!(RestoreMode::parse("nope"), None);
    assert_eq!(RestoreMode::from_entries(&[]).0, RestoreMode::Agents);
    assert_eq!(RestoreMode::from_entries(&[r("off")]).0, RestoreMode::Off);
    assert_eq!(RestoreMode::from_entries(&[r("off"), r("")]).0, RestoreMode::Agents);
    let (mode, warnings) = RestoreMode::from_entries(&[r("layout"), r("bogus")]);
    assert_eq!((mode, warnings.len()), (RestoreMode::Layout, 1));
    assert_eq!(RestoreMode::Off.as_str(), "off");
}

#[test]
fn encode_then_decode_round_trips() {
    assert_eq!(decode(&encode(&sample())), Some(sample()));
}

#[test]
fn decode_rejects_garbage_and_other_versions() {
    assert_eq!(decode(""), None);
    assert_eq!(decode("{\"version\": 1, \"sessions\": [tru"), None);
    assert_eq!(decode("{\"version\": 2}"), None);
}

#[test]
fn decode_fills_missing_fields() {
    let state = decode("{\"version\": 1, \"sessions\": [{}]}").unwrap();
    assert_eq!(state.active, None);
    assert_eq!(state.sessions, vec![SavedSession::default()]);
}

#[test]
fn launches_follow_the_mode() {
    let s = sample();
    assert!(launches(&s, RestoreMode::Off).is_empty());
    let layout = launches(&s, RestoreMode::Layout);
    assert_eq!(layout.len(), 2);
    assert!(layout.iter().all(|l| !l.run_preset));
    let agents = launches(&s, RestoreMode::Agents);
    assert_eq!(agents.iter().map(|l| l.run_preset).collect::<Vec<_>>(), [false, true]);
    assert_eq!(agents[1].saved, s.sessions[1]);
}

#[test]
fn state_path_is_in_application_support() {
    assert_eq!(
        state_path(Path::new("/Users/me")),
        PathBuf::from("/Users/me/Library/Application Support/fuxx/sessions.json")
    );
}
```

- [ ] **Step 2: Run** `cargo test restore`. Expected: compile errors.
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (137), 0 warnings.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m7): session-restore setting and the saved-sessions file format"`

### Task 4: Sessions in folders, agent presets in the menu — ⬜

**Files:** Modify `src/terminal_view.rs`, `src/workspace.rs`, `src/main.rs`.

**Interfaces:**
- `TerminalView::spawn(window, cx, cwd: Option<&Path>, startup: Option<&str>) -> Result<Entity<Self>, String>`: the shell starts in `cwd` (`pty::shell_command(cwd)`); if `startup` is `Some(cmd)`, write `format!("{cmd}\r")` to the PTY right after spawning (the shell reads it as typed-ahead input).
- `Session` gains `name: Option<String>`, `project: Option<PathBuf>`, `preset: Option<Preset>`; `label()` = `groups::auto_name(name, title, preset name)`.
- `Workspace::open_session(&mut self, launch: Launch, window, cx)`: start directory = `launch.saved.project` if it `is_dir()`, else `None` (home) with one `[fuxx] session: folder <path> is gone, starting in home` line; startup command = the preset's command only if `launch.run_preset`; inserted at `groups::insert_index(..)` and made active.
- `NewSession` (⌘T): a `Launch` with the active session's `project`, no preset.
- New actions (in `workspace`): `NewSessionInFolder` (`cmd-o`): `cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Open".into()) })`, awaited in `cx.spawn_in(window, …)`; a chosen folder → `open_session` with that project; cancel → nothing.
- `LaunchPreset1` … `LaunchPreset9` (`ctrl-cmd-1` … `ctrl-cmd-9`): preset *n* of `Appearance.presets` (ignored if absent) → `open_session` with the active session's project, that preset, `run_preset: true`.
- `main.rs` menus (bindings first, as today), File menu: New Session · New Session in Folder… · **New Agent Session ▸** (a submenu, one item per preset in `Appearance.presets`, named by the preset, bound to `LaunchPresetN`) · separator · Close Session · separator · Preferences…. The appearance must be loaded **before** `set_menus` now (move `appearance::load` up).

- [ ] **Step 1:** Implement. No new unit tests (GPUI wiring); the pure parts were tested in Tasks 1–3.
- [ ] **Step 2: Run** `cargo build` and `cargo test`. Expected: 0 warnings, 137 passed.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m7): new session in a folder, agent presets in File menu"`

### Task 5: The grouped sidebar, renaming and reordering — ⬜

**Files:** Create `src/name_input.rs` · Modify `src/workspace.rs`, `src/main.rs` (`mod name_input;`, menus, bindings).

**Interfaces:**
- `NameInput` (a GPUI view, `Focusable`, `EventEmitter<NameInputEvent>`): `pub fn new(text: String, window, cx) -> Self`; `pub enum NameInputEvent { Confirm(String), Cancel }`.
  - Implements `EntityInputHandler` (typed text, ⌥-accents, the emoji picker and IME marked text all arrive as `replace_text_in_range` / `replace_and_mark_text_in_range`); the caret is always at the end — insertion appends.
  - Keys: Backspace removes the last `char`; ⌘Backspace clears; Enter → `Confirm(text.trim())`; Escape → `Cancel`; losing focus → `Cancel`.
  - Registers its input handler during paint via `window.handle_input(&focus_handle, ElementInputHandler::new(bounds, entity), cx)` as in `examples/input.rs`.
- Workspace:
  - `collapsed: Vec<PathBuf>`; the sidebar renders `groups::rows(..)`. Header: arrow ▾/▸, `groups::group_label`, `ring(groups::rolled_up(..))`, a **+** that opens a session in that project (its click must not also toggle the header). Header click toggles collapse. Grouped session rows are indented.
  - Selecting a session by any route (click, ⌘1…9, next/prev, new, restore) removes its project from `collapsed`.
  - `renaming: Option<(EntityId, Entity<NameInput>)>` keyed by the session view's entity id. Double-click (`click_count() == 2`) or `RenameSession` (`cmd-r`) opens it on that row and focuses it; `Confirm` sets `name` (empty → `None`); both events close it and refocus the active terminal.
  - Drag: each session row `.on_drag(DraggedSession { view: EntityId, label }, …)` with a small label preview; `.on_drop` on rows moves the dragged session to the drop row's index **only if** `groups::same_group` holds at drop time (look the dragged one up by entity id); `.drag_over::<DraggedSession>` tints the row.
  - `MoveSessionUp` / `MoveSessionDown` (menu only): swap with the neighbour if `same_group`, else nothing.
- Menus: File gains Rename Session… (⌘R), Move Session Up, Move Session Down after the presets submenu.

- [ ] **Step 1:** Implement. No new unit tests (GPUI); ordering rules are Task 1's.
- [ ] **Step 2: Run** `cargo build` and `cargo test`. Expected: 0 warnings, 137 passed.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m7): grouped sidebar, rename box, drag and menu reordering"`

### Task 6: Save and restore the sessions; the Preferences row — ⬜

**Files:** Modify `src/workspace.rs`, `src/preferences.rs`, `src/prefs.rs` (if a pure helper is needed).

**Interfaces:**
- `Workspace::new`: `mode = Appearance.restore`. If not `Off` and `$HOME` is set and `decode(read(state_path))` succeeds with at least one session → `open_session` for each of `launches(..)` in order, then `collapsed` and the active index from the file (clamped). Otherwise one plain session. Each fallback reason logs one `[fuxx] restore: …` line (missing file is not an error: no line).
- `fn save(&self)`: builds `SavedState { version: VERSION, active, collapsed, sessions }` from the sessions (`name`, `project`, `preset`) and writes `encode(..)` with `settings::write_atomic(state_path)`. Failure → one `[fuxx] restore: could not save …` line. Called after open, close, rename, move, select (any route) and collapse/expand — never in the middle of restoring (save once at the end of `new`).
- Closing the last session: `save()` (empty list) **then** `window.remove_window()`.
- Preferences window: a **Restore sessions** row with three buttons **Off** / **Layout** / **Layout + agents**; the current mode is highlighted. A click writes `session-restore` like the other rows (the default `agents` removes the line; the others write `off` / `layout`) and reloads. Grow the window height if the rows no longer fit (measure, as in M6b).

- [ ] **Step 1:** Implement. No new unit tests (the format and modes are Task 3's).
- [ ] **Step 2: Run** `cargo build` and `cargo test`. Expected: 0 warnings, 137 passed.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m7): save and restore sessions; Restore sessions in Preferences"`

### Task 7: On screen — 👀 ⬜

Done together with the M6b and M8 checks (your choice, 2026-10-27). Run `cargo run` and report.

- [ ] **Groups:** ⌘O opens a folder picker; the new session's `pwd` is that folder and it sits under a header with that folder's name. ⌘T from it joins the same group. The **+** on the header adds one more. Clicking the header closes/opens the group; ⌘N to a hidden session opens it. The header ring shows the most urgent of its sessions.
- [ ] **Rename:** double-click a session (or ⌘R), type `🖕 api` using the emoji picker (⌃⌘Space), Enter → the sidebar, window title and notifications use it. Escape cancels. An empty name goes back to the automatic one.
- [ ] **Reorder:** drag a session onto another in the same group: they swap places. Dragging into another group does nothing. File → Move Session Up/Down work.
- [ ] **Presets:** File → New Agent Session lists claude, opencode, codex (with ⌃⌘1/2/3). ⌃⌘1 opens a session in the current group and starts `claude`. Add `agent-preset = hello: echo hi` to the settings file, restart: "hello" is the 4th entry and prints `hi`.
- [ ] **Restore:** quit with ⌘Q and reopen: same sessions, names, order, groups, closed groups and current session; claude starts again. Set Preferences → Restore sessions → Layout, restart: the same sessions but claude doesn't start. Off: one new session. Close every session with `exit`: next launch starts one new session.
- [ ] **Unchanged:** M5/M6/M6b behaviours (selection, links, Mac keys, rings, Preferences).

### Task 8: Update the docs — ⬜

- [ ] Milestone note 47 ✅ with the checklist ticked; this plan's tasks ✅; roadmap M7 ✅; tracker.
- [ ] `STRUCTURE.md`: `groups.rs`, `presets.rs`, `restore.rs`, `name_input.rs`.
- [ ] Tech Stack: `serde`/`serde_json` added (from the lockfile, via GPUI). API Fact Sheet: `prompt_for_paths`, `MenuItem::submenu`, `on_drag`/`on_drop`, `EntityInputHandler`.
- [ ] Progress Log entry.

## Verification log

_(filled in as tasks complete)_

## Known limitations

- The rename box always types at the end (no caret movement or selection).
- Restored sessions start in their group's folder, not wherever the shell had `cd`'d to.
- Presets and the restore mode are read at startup; Preferences changes to the restore mode apply from the next launch.

## Related

- [[47 - Milestone 7 - Session Ergonomics]] · [[40 - Roadmap]] · [[00 - Plan Tracker (Start Here)]]
