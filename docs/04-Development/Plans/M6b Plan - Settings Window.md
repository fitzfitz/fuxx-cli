---
tags: [development, plan, milestone]
milestone: M6b
status: not-started
updated: 2026-10-27
---

# M6b Plan — Settings Window (File → Preferences… / cmd-,)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[46b - Milestone 6b - Settings Window]] (the spec) |
| **Status** | ⬜ Not started |
| **Tasks done** | 0 / 6 |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | **Yours:** File → Preferences… + cmd-, (2026-10-27); **D9** = pick from the theme list, no typing. The rest are the AI's calls listed in the spec. |
| **Needs your eyes** | 👀 Task 5: menus, the window, live theme/font/size/ligature changes, the saved file |

**Goal:** A menu bar and a Preferences window that changes theme, font, size and ligatures live for every
session, and saves only the changed line of `~/.config/fuxx/config`.

**Architecture:**

```
 File → Preferences… / cmd-, ──► OpenPreferences (global action) ──► open or activate PreferencesView window
 PreferencesView click ──► prefs::* (pure: choices, size step, feature lines)
                       └► settings::set_key(text, key, values) ──► settings::write_atomic(path)
                       └► appearance::reload(cx) ──► cx.set_global(Appearance) ──► observers:
                              TerminalView: core.set_theme(theme) + notify      (font/size: read each prepaint)
                              Workspace: notify (sidebar UiColors)
```

**Tech Stack:** No new crates. GPUI at the pinned rev: `cx.set_menus(Vec<Menu>)`, `MenuItem::action`,
`cx.open_window` → `WindowHandle`, `window.activate_window()`, `window.remove_window()`, `uniform_list`,
`.on_click(cx.listener(..))`, `cx.observe_global::<G>`, `cx.on_window_closed`.

**Spec:** [[46b - Milestone 6b - Settings Window]] · [[46 - Milestone 6 - Fonts and Config]] · [[12 - Scope]]

## Global Constraints

- **Inherited:** everything from the M0–M6 plans. **No new crates.**
- **Gates:** 0 warnings from our code; `cargo fmt --check` clean. Final count **116 tests** (106 today).
- **Behaviour:** exactly the spec's "How it behaves", and nothing under "Not in M6b".
- **Writing the settings file:**
  - Only the Preferences window writes, and only `~/.config/fuxx/config` (or `$XDG_CONFIG_HOME/fuxx/config`).
  - It changes only the lines of the key being set; every other byte stays as it was.
  - The write is atomic: a temp file in the same directory, then a rename.
  - The directory is created if it's missing.
  - It never writes an included file.
- **Failures:** a failed write or reload never crashes. Log one `[fuxx] preferences: …` line and keep the old look.
- **Pure logic:** `settings::set_key`, `prefs.rs` and `TerminalCore::set_theme` are unit-tested. The GPUI view and menus are verified by build and Task 5.
- **Bindings before menus:** `cx.bind_keys` runs before `cx.set_menus`, because menu shortcuts are taken from the bindings at that moment. `cmd-,` is bound with no context, so the menu shows it.

## Review Focus

1. **Saving damages the user's file** (comments, overrides or other keys lost; a partial write) → `set_key_replaces_in_place_and_keeps_everything_else`, `set_key_with_no_values_removes_the_key`, `write_atomic_replaces_the_file` (Task 1).
2. **A theme change doesn't reach sessions that are already open** (each core stored its own copy) → `set_theme_changes_snapshot_colors` (Task 3); Task 5 checks with several sessions.
3. **The font list shows proportional fonts**, or is slow to open (hundreds of fonts) → `prefs` monospace filter tests (Task 2). The list is computed once per window opening; Task 5 times it.
4. **Size or ligature changes write invalid values** → `step_size_clamps_and_rounds`, `feature_lines_toggle_only_calt` (Task 2).
5. **Two Preferences windows, or closing the terminal leaves fuxx running with only Preferences** → the window handle is reused; `on_window_closed` for the main window quits (Task 4); Task 5 checks both.

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `src/settings.rs` | Modify | `set_key`, `write_atomic` | 1 |
| `src/prefs.rs` | Create | Pure choices and edits: `theme_choices`, `font_choices`, `is_monospace`, `step_size`, `ligatures_on`, `feature_lines` | 2 |
| `src/terminal.rs` | Modify | Theme becomes shared and mutable: `set_theme`, `theme() -> Theme` | 3 |
| `src/appearance.rs` | Modify | `reload(cx)`; `Appearance` carries `theme_name`, `font_settings`, `settings_path`, `user_theme_dir` | 3 |
| `src/terminal_view.rs`, `src/workspace.rs` | Modify | Observe `Appearance` (theme → core, notify) | 3 |
| `src/preferences.rs` | Create | `PreferencesView` (GPUI), `open_or_activate(cx)` | 4 |
| `src/main.rs` | Modify | `OpenPreferences` action, `cmd-,` binding, `set_menus`, quit when the main window closes | 4 |

---

### Task 1: Saving one setting safely — ⬜

**Files:** Modify `src/settings.rs`.

**Interfaces:**
- `pub fn set_key(text: &str, key: &str, values: &[String]) -> String`
  - Lines whose `parse`d key equals `key` are removed. Comments and lines with other keys stay byte-for-byte.
  - The new `key = value` lines (one per value, in order) go where the **first** removed line was. If `key` wasn't there, they're appended at the end; when the text is non-empty and doesn't end in a newline, add one first.
  - An empty `values` only removes.
  - The result ends with `\n` unless it is empty.
- `pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()>`
  - `create_dir_all(parent)`, then write `<path>.<pid>.tmp` and `rename` it over `path`.
  - The tmp file is removed on error.

- [ ] **Step 1: Failing tests:**

```rust
#[test]
fn set_key_replaces_in_place_and_keeps_everything_else() {
    let text = "# my comment\nbackground = #000000\ntheme = Nord\nfont-size = 14\n";
    assert_eq!(set_key(text, "theme", &["Dracula".into()]), "# my comment\nbackground = #000000\ntheme = Dracula\nfont-size = 14\n");
    let multi = "font-feature = -calt\nx = 1\nfont-feature = ss01\n";
    assert_eq!(set_key(multi, "font-feature", &["ss01=1".into()]), "font-feature = ss01=1\nx = 1\n");
}

#[test]
fn set_key_with_no_values_removes_the_key() {
    assert_eq!(set_key("theme = Nord\n# keep\n", "theme", &[]), "# keep\n");
    assert_eq!(set_key("", "theme", &[]), "");
}

#[test]
fn set_key_appends_when_missing() {
    assert_eq!(set_key("x = 1", "theme", &["Nord".into()]), "x = 1\ntheme = Nord\n");
    assert_eq!(set_key("", "font-size", &["15".into()]), "font-size = 15\n");
}

#[test]
fn write_atomic_replaces_the_file() {
    let dir = std::env::temp_dir().join(format!("fuxx-write-atomic-{}", std::process::id()));
    let path = dir.join("sub/config");
    write_atomic(&path, "a = 1\n").unwrap();
    write_atomic(&path, "a = 2\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 2\n");
    assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1); // no tmp left behind
    std::fs::remove_dir_all(&dir).unwrap();
}
```

- [ ] **Step 2: Run** `cargo test settings`. Expected: compile errors.
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run** `cargo test`. Expected: `110 passed` (dead code allowed until Task 4).
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m6b): change one setting in the settings file, written atomically"`

### Task 2: `prefs.rs` — the window's choices, without GPUI — ⬜

**Files:** Create `src/prefs.rs` · Modify `src/main.rs` (`mod prefs;`).

**Interfaces:**
- `pub const DEFAULT_CHOICE: &str = "Default";`
- `pub fn theme_choices(user_themes: &[String]) -> Vec<String>`
  - `DEFAULT_CHOICE` first, then the union of `theme::BUILTIN_THEMES` and `user_themes`.
  - Sorted case-insensitively, with duplicates removed.
- `pub fn font_choices(installed: &[String], is_mono: &dyn Fn(&str) -> bool, current: Option<&str>) -> Vec<String>`
  - `DEFAULT_CHOICE` first, then the installed families for which `is_mono` is true.
  - Names starting with `.` (system-private) are skipped. The list is sorted case-insensitively.
  - `current` is always included, even if not detected as mono.
- `pub fn is_monospace(i_advance: f32, m_advance: f32) -> bool` returns `(i - m).abs() < 0.01 && m > 0.0`.
- `pub fn step_size(size: f32, delta: i32) -> f32` returns `(size.round() + delta as f32).clamp(4.0, 200.0)`.
- `pub fn ligatures_on(features: &[(String, u32)]) -> bool` is false iff the last `calt` entry has value 0.
- `pub fn feature_lines(features: &[(String, u32)], ligatures: bool) -> Vec<String>`
  - Every non-`calt` feature as `tag=value`, in order.
  - Then `-calt` if `!ligatures`.

- [ ] **Step 1: Failing tests:**

```rust
#[test]
fn theme_choices_merge_sort_and_dedupe() {
    let c = theme_choices(&["Rose Pine".into(), "Nord".into()]);
    assert_eq!(c[0], DEFAULT_CHOICE);
    assert_eq!(c.iter().filter(|n| n.as_str() == "Nord").count(), 1);
    assert!(c.contains(&"Rose Pine".to_string()) && c.contains(&"Dracula".to_string()));
    assert_eq!(c.len(), 1 + 6 + 1);
}

#[test]
fn font_choices_keep_mono_and_current() {
    let installed: Vec<String> = ["Menlo", "Helvetica", ".SFNS", "MesloLGS NF"].iter().map(|s| s.to_string()).collect();
    let mono = |n: &str| n == "Menlo" || n == "MesloLGS NF";
    assert_eq!(font_choices(&installed, &mono, Some("Helvetica")), vec![DEFAULT_CHOICE, "Helvetica", "Menlo", "MesloLGS NF"]);
    assert_eq!(font_choices(&installed, &mono, None), vec![DEFAULT_CHOICE, "Menlo", "MesloLGS NF"]);
}

#[test]
fn is_monospace_compares_advances() {
    assert!(is_monospace(8.4, 8.4) && !is_monospace(3.9, 12.0) && !is_monospace(0.0, 0.0));
}

#[test]
fn step_size_clamps_and_rounds() {
    assert_eq!([step_size(13.0, 1), step_size(13.4, -1), step_size(4.0, -1), step_size(200.0, 1)], [14.0, 12.0, 4.0, 200.0]);
}

#[test]
fn feature_lines_toggle_only_calt() {
    let f = vec![("ss01".to_string(), 1), ("calt".to_string(), 0)];
    assert!(!ligatures_on(&f) && ligatures_on(&[]));
    assert_eq!(feature_lines(&f, true), vec!["ss01=1"]);
    assert_eq!(feature_lines(&f, false), vec!["ss01=1", "-calt"]);
}
```

- [ ] **Step 2: Run** `cargo test prefs`. Expected: compile errors.
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run** `cargo test`. Expected: `115 passed`.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m6b): pure choices for the preferences window"`

### Task 3: Live reload of the look — ⬜

**Files:** Modify `src/terminal.rs`, `src/appearance.rs`, `src/terminal_view.rs`, `src/workspace.rs`.

**Interfaces:**
- `TerminalCore`:
  - The theme lives in `Arc<Mutex<Theme>>`, shared by all clones.
  - `pub fn set_theme(&self, theme: Theme)`.
  - `pub fn theme(&self) -> Theme` returns it by value; update its callers.
  - `snapshot()` and `color()` read the current theme.
- `Appearance` gains these fields, filled in by `load`:
  - `pub theme_name: Option<String>`: the last `theme` entry's value.
  - `pub font_settings: fonts::FontSettings`.
  - `pub settings_path: Option<PathBuf>`.
  - `pub user_theme_dir: Option<PathBuf>`.
- `pub fn reload(cx: &mut App)`: `let a = load(cx); cx.set_global(a);`. `load` already never fails.
- `TerminalView::new`: `cx.observe_global::<Appearance>(|this, cx| { let t = cx.global::<Appearance>().theme; this.core.set_theme(t); cx.notify(); })`, with the `Subscription` stored in the view.
- `Workspace::new`: `cx.observe_global::<Appearance>(|_, cx| cx.notify())`, also stored.

- [ ] **Step 1: Failing test** in `terminal.rs`:

```rust
#[test]
fn set_theme_changes_snapshot_colors() {
    let (tx, mut rx) = unbounded();
    let core = TerminalCore::spawn(sh("printf 'x'; sleep 1"), TermDims { cols: 10, rows: 2 }, tx, Theme::default()).unwrap();
    assert!(wait_for(&mut rx, |_| screen_text(&core).contains('x')));
    let clone = core.clone();
    let mut t = Theme::default();
    t.foreground = crate::theme::parse_color("#123456").unwrap();
    clone.set_theme(t);
    assert_eq!(core.snapshot().lines[0][0].fg, t.foreground); // every clone sees it
    assert_eq!(core.theme(), t);
}
```

- [ ] **Step 2: Run** `cargo test set_theme`. Expected: compile error (`set_theme` missing).
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run** `cargo test`. Expected: `116 passed`.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m6b): theme and font changes reach every open session live"`

### Task 4: Menus and the Preferences window — ⬜

**Files:** Create `src/preferences.rs` · Modify `src/main.rs`.

**Interfaces:**
- **Action:** `main.rs` defines `actions!(fuxx, [Quit, OpenPreferences])`.
- **Key bindings:** add `KeyBinding::new("cmd-,", OpenPreferences, None)` to the existing `bind_keys`, **before** `set_menus`.
- **Global handler:** `cx.on_action(|_: &OpenPreferences, cx| preferences::open_or_activate(cx))`.
- **Menus**, set with `cx.set_menus`:
  - `Menu::new("fuxx").items([MenuItem::action("Quit fuxx", Quit)])`
  - `Menu::new("File").items([MenuItem::action("New Session", NewSession), MenuItem::action("Close Session", CloseSession), MenuItem::separator(), MenuItem::action("Preferences…", OpenPreferences)])`
- **Quit with the main window:** keep the main window's `WindowHandle`. `cx.on_window_closed(move |cx, id| if id == main_id { cx.quit() }).detach()`. Closing the terminal window then quits even while Preferences is open.
- **`preferences::open_or_activate(cx: &mut App)`:**
  - A global `PreferencesWindow(Option<WindowHandle<PreferencesView>>)` holds the handle.
  - If `handle.update(cx, |_, w, _| w.activate_window())` is `Ok`, stop there.
  - Otherwise open a new window: `WindowKind::Normal`, `is_resizable: false`, `is_minimizable: false`, title "Preferences", centered, 560×600. Store the new handle.
- **`PreferencesView` creation:**
  - Hold a `FocusHandle` and focus it at creation.
  - Compute `themes = prefs::theme_choices(names in user_theme_dir)` **once**.
  - Compute `fonts = prefs::font_choices(&all_font_names, &is_mono, current)` **once**.
    - `is_mono` resolves each family with `text_system.resolve_font(&font(name))`, then compares `advance(id, px(13), 'i')` with `'m'` through `prefs::is_monospace`.
    - An `Err` from either advance counts as not mono.
  - `.on_action(|_: &CloseSession, window, _| window.remove_window())` handles cmd-W.
- **`PreferencesView` render:** four sections, laid out with styled `div`s.
  - **Theme:** a `uniform_list`, about 180 px tall. The row for the current theme (`Appearance.theme_name`, or "Default" when it's `None`) is highlighted.
  - **Font:** the same, built from `fonts`.
  - **Size:** `[−] N pt [+]`.
  - **Ligatures:** an `[On|Off]` toggle.
  - **Colors:** taken from the `UiColors` of `Appearance.theme`, so the window matches the theme.
  - **Live state:** observe `Appearance` and notify, so the highlighted rows follow changes.
- **Each click calls `apply(cx, key, values)`:**
  1. Read the current file text from `Appearance.settings_path`; if it's missing, use `""`.
  2. `settings::set_key`.
  3. `settings::write_atomic`.
     - On `Err`, log one `[fuxx] preferences: …` line and return without reloading.
  4. `appearance::reload(cx)`.
- **The values each click writes:**

  | Click | Key | Values |
  |---|---|---|
  | Theme | `"theme"` | `[name]`, or `[]` for Default |
  | Font | `"font-family"` | `[name]`, or `[]` for Default |
  | − / + | `"font-size"` | `[step_size(cur, ±1) formatted without a trailing ".0"]` |
  | Ligatures | `"font-feature"` | `prefs::feature_lines(&cur_features, !on)` |

  If `settings_path` is `None` (no `$HOME`), log one `[fuxx] preferences: …` line and do nothing.

> **No new unit tests:** this task is GPUI wiring over the tested functions of Tasks 1–3. It's verified by the build and by Task 5.

- [ ] **Step 1: Implement** as specified.
- [ ] **Step 2: Gates.**
  - `cargo build`: **0 warnings** from our code.
  - `cargo test`: `116 passed`.
  - `cargo fmt --check`: clean.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m6b): menu bar and the Preferences window (cmd-,)"`

### Task 4b: Bundle JetBrains Mono as the default font — ⬜

*Added 2026-10-27 on request, after the ligature check showed no change with the installed fonts. Spec: "Also in M6b: JetBrains Mono comes with fuxx".*

**Files:** Add `assets/fonts/JetBrainsMono-{Regular,Bold,Italic,BoldItalic}.ttf` and `assets/fonts/LICENSE-JetBrainsMono-OFL.txt` (from the official v2.304 release zip, sha256 `6f6376c6…f7bbf`) · Modify `src/fonts.rs`, `src/appearance.rs`, `src/preferences.rs` (if it calls the registration).

**Interfaces:**
- `fonts::register_bundled_fonts(installed: &[String]) -> bool` replaces `register_symbols_font()`. It writes each bundled file to `~/Library/Caches/fuxx/` (same temp + rename + size check as today) and registers it for this process, cached once per process (`OnceLock`). It skips the four JetBrains Mono files when `"JetBrains Mono"` is already in `installed`, so your own copy wins and CoreText doesn't report a duplicate. Returns whether every file it tried was registered. Failures log one `[fuxx] font: …` line each.
- `appearance::load` calls it with `all_font_names()` taken **before** registering, then takes the font list again for `gpui_font`. `gpui_font` keeps its rule (JetBrains Mono if listed, else Menlo), so the bundled copy becomes the default by being listed.

- [ ] **Step 1: Failing tests** (headless text system, as in `bundled_symbols_draw_the_apple_icon`):

```rust
#[test]
fn bundled_jetbrains_mono_is_listed_and_the_default() {
    let ts = gpui_platform::current_platform(true).text_system();
    assert!(register_bundled_fonts(&ts.all_font_names()));
    let installed = ts.all_font_names();
    assert!(installed.iter().any(|f| f == "JetBrains Mono"));
    let d = gpui_font(&FontSettings { families: vec![], size: 13.0, features: vec![] }, &installed);
    assert_eq!(d.family.as_ref(), "JetBrains Mono");
}

#[test]
fn jetbrains_mono_ligatures_follow_calt() {
    // "->" is drawn with different glyphs when contextual alternates are on.
    use gpui::{FontRun, px};
    let ts = gpui_platform::current_platform(true).text_system();
    assert!(register_bundled_fonts(&ts.all_font_names()));
    let glyphs = |features: Vec<(String, u32)>| {
        let font = gpui_font(&FontSettings { families: vec!["JetBrains Mono".into()], size: 13.0, features }, &ts.all_font_names());
        let id = ts.font_id(&font).unwrap();
        let layout = ts.layout_line("->", px(13.0), &[FontRun { len: 2, font_id: id }]);
        layout.runs.iter().flat_map(|r| r.glyphs.iter().map(|g| g.id)).collect::<Vec<_>>()
    };
    assert_ne!(glyphs(vec![]), glyphs(vec![("calt".into(), 0)]));
}
```

- [ ] **Step 2: Run** `cargo test fonts`. Expected: compile errors.
- [ ] **Step 3: Implement** as specified; update the module docs and the `bundled_symbols_draw_the_apple_icon` test to the new function.
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (120), 0 warnings, fmt clean.
- [ ] **Step 5: Commit:** `git add assets src && git commit -m "feat(m6b): bundle JetBrains Mono as the default font"`

### Task 5: On screen — 👀 ⬜

Run `cargo run` and report.

- [ ] **Menus:** fuxx (Quit fuxx ⌘Q) and File (New Session ⌘T, Close Session ⌘W, Preferences… ⌘,).
- [ ] **Opening:** cmd-, opens Preferences, quickly (note if the font list takes more than about a second). cmd-, again brings the same window to the front rather than opening a second one. cmd-W closes it.
- [ ] **Theme:** with two sessions open, click "Catppuccin Latte". Both sessions and the sidebar change at once, and the window shows it highlighted. Click "Default": back to the default colors.
- [ ] **Font, size, ligatures:**
  - Pick "MesloLGS NF". The text changes live.
  - Press + a few times: the text grows and the grid re-fits, so `tput cols` changes.
  - Press − back.
  - The ligatures switch toggles (no visible change with fonts that have no ligatures).
- [ ] **Saving:**
  - Open `~/.config/fuxx/config`. Your other lines and comments are untouched, and only `theme`, `font-family`, `font-size` and `font-feature` changed.
  - Quit and reopen fuxx: the same look.
- [ ] **Your override still wins:** with `background = #000000` in the file, switching themes keeps the black background.
- [ ] **Quitting:** with Preferences open, close the terminal window. fuxx quits.
- [ ] **Unchanged:** M5 and M6 behaviours (selection, links, Mac keys, rings).
- [ ] **JetBrains Mono (added 2026-10-27):** with no `font-family` line (or "Default" picked), the text is JetBrains Mono. `echo "-> => != >= ==="` shows arrows/≠/≥ with Ligatures On and plain characters with Off.
- [ ] **Session name (added on request 2026-10-27):** a new session shows `N  🖕` in the sidebar and `fuxx — 🖕` in the window title, and the emoji draws in color (not a `?` box). Implemented in `src/workspace.rs` `Session::label`; no unit test (needs a live GPUI session), checked on screen here.

### Task 6: Update the docs — ⬜

- [ ] [[46b - Milestone 6b - Settings Window]]: tick the checklist; **Status:: ✅ Done**.
- [ ] [[40 - Roadmap]]: M6b → ✅ (link the note).
- [ ] [[00 - Plan Tracker (Start Here)]]: M6b row → ✅ 6/6; **Where we are** → M7, plan to be written.
- [ ] [[46 - Milestone 6 - Fonts and Config]] / [[12 - Scope]]: note that the Preferences window now writes the settings file.
- [ ] [[31 - API Fact Sheet (pinned rev)]]: menus (bind keys before `set_menus`; shortcuts come from context-free bindings), the second window (`WindowHandle`, `activate_window`, `remove_window`, `on_window_closed`), `uniform_list`, `observe_global`.
- [ ] `STRUCTURE.md`: `prefs.rs`, `preferences.rs`.
- [ ] [[60 - Progress Log]]: entry "M6b complete: settings window".
- [ ] Commit docs.

## Verification log

| Date | Check | Result | Evidence |
|---|---|---|---|
| 2026-10-27 | GPUI at the pinned rev: `set_menus`/`Menu`/`MenuItem` (shortcuts from context-free bindings at `set_menus` time), `open_window`→`WindowHandle` (`update` fails after close), `activate_window`, `remove_window`, `on_window_closed`, `QuitMode::LastWindowClosed` needs ALL windows closed, `uniform_list`, `on_click`+`cx.listener`, `observe_global`, no built-in Button/Checkbox | ✅ | Research agent, file:line refs |
| — | Task code itself | ⚠️ not pre-compiled (signatures and tests) | Verified per task during execution |

## Known limitations

| Limitation | Lands in |
|---|---|
| No typing (hex colors, font names) and no per-color pickers (D9) | Later, if wanted |
| A hand edit to the settings file still needs a restart | Later, if wanted |
| If an included file sets the same key, it overrides what Preferences wrote (includes load after the main file) | By design; noted |
| Monospace detection compares the widths of 'i' and 'm'; an odd font could be misjudged | Revisit if it bites |

## Related

[[00 - Plan Tracker (Start Here)]] · [[M6 Plan - Fonts and Config]] · [[40 - Roadmap]] · [[12 - Scope]]
