---
tags: [development, plan, milestone]
milestone: M6
status: not-started
updated: 2026-10-27
---

# M6 Plan — Fonts and Config (read Ghostty's config; theme; prompt icons)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[46 - Milestone 6 - Fonts and Config]] (the spec: every behaviour below is listed there) |
| **Status** | ⬜ Not started |
| **Tasks done** | 0 / 7 |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | The calls in [[46 - Milestone 6 - Fonts and Config]] (made by the AI from the mockup and Ghostty 1.3.1's docs; you said "skip") |
| **Needs your eyes** | 👀 Task 6: your prompt icons and colors; a Ghostty theme switch |

**Goal:** fuxx starts with the font, size, ligature setting and colors from the user's Ghostty config
(Ghostty's defaults when there is none), prompt icons always draw, and the sidebar follows the theme.

**Architecture:**

```
 startup (main.rs) ── appearance::load(cx) ──────────────────────────────────────────────┐
   ghostty_config::config_paths(home, xdg) → load(paths, read) → Vec<Entry>  (includes) │
   theme::resolve(&entries, dark, read_theme) → Theme   (defaults → theme file → user)  │
   fonts::from_entries(&entries) → FontSettings → fonts::gpui_font(&s, &installed)      │
   fonts::register_symbols_font()  (bundled Symbols Nerd Font → CoreText, process scope)│
   cx.set_global(Appearance { theme, font, font_size }) ◄───────────────────────────────┘
 TerminalView::spawn → TerminalCore::spawn(.., theme) → grid::snapshot(term, &theme) → paint
 Workspace sidebar → theme.ui() → UiColors (sidebar bg, rows, rings)
```

**Tech Stack:** No new crates. A bundled font file `assets/fonts/SymbolsNerdFontMono-Regular.ttf`
(Nerd Fonts v3, MIT, ~2.6 MB) plus its licence. Two CoreFoundation/CoreText C calls declared with
`extern "C"` (the frameworks are already linked by GPUI).

**Spec:** [[46 - Milestone 6 - Fonts and Config]] · `mockups/2-daily-driver.html` (#5, #6) · [[40 - Roadmap]] · [[12 - Scope]]

## Global Constraints

- Everything from the M0–M5 plans. **No new crates.** 0 warnings from our code; `cargo fmt --check` clean. Final count: **101 tests** (85 today).
- Behaviour is exactly the list in [[46 - Milestone 6 - Fonts and Config]]; nothing under "Not in M6".
- **fuxx never fails to start because of a config.** Missing files, unreadable files, bad values, unknown keys, missing theme or font → skipped (one `eprintln!("[fuxx] config: …")` line each), defaults used.
- **fuxx never writes Ghostty's files.** The only file fuxx writes is the cached font copy (Task 4).
- Ghostty defaults (verified with `ghostty +show-config --default`, 1.3.1): background `#282c34`, foreground `#ffffff`, font size `13`, palette 0–15 = `1d1f21 cc6666 b5bd68 f0c674 81a2be b294bb 8abeb7 c5c8c6 666666 d54e53 b9ca4a e7c547 7aa6da c397d8 70c0b1 eaeaea`. Colors 16–255 stay the xterm cube/gray ramp (unchanged code).
- Pure logic (`ghostty_config.rs`, `theme.rs`, `fonts.rs` except registration) takes text/paths/closures, not the real filesystem, so it is unit-tested.
- `unsafe` is allowed only in `fonts::register_symbols_font`, with a `// SAFETY:` comment.

## Review Focus

1. **A broken or odd Ghostty config stops fuxx starting or panics** (bad hex, `palette = 300=…`, include loop, missing theme, empty file) → tests `bad_values_are_skipped`, `include_cycles_stop`, `empty_value_resets_and_missing_theme_keeps_defaults` (Tasks 1–2); Task 6 checks a deliberately broken config.
2. **Precedence wrong**: theme beats the user's own colors, or an earlier file beats a later one → `a_named_theme_loads_and_user_values_override_it`, `later_files_win_and_includes_follow_their_file` (Tasks 1–2).
3. **The bundled icon font isn't used** (macOS ignores in-memory fonts — verified by spike 2026-10-27) → `bundled_symbols_draw_the_apple_icon` headless test (Task 4); Task 6 checks your prompt.
4. **A font that isn't installed silently becomes Helvetica** (GPUI's `resolve_font` falls back to a proportional font) → `gpui_font_skips_missing_families` (Task 4).
5. **Readability**: selection or sidebar text unreadable on light themes → `ui_colors_follow_light_and_dark_themes` (Task 3); Task 6 tries a light theme.

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `src/ghostty_config.rs` | Create | `Entry`, `parse`, `config_paths`, `load` (includes) | 1 |
| `src/theme.rs` | Create | `Theme` (+ Ghostty defaults), `parse_color`, `apply`, `resolve`, `theme_dirs`; `mix`, `UiColors` | 2, 3 |
| `src/colors.rs` | Modify | Palette and default fg/bg come from a `&Theme`; drop the fixed constants | 3 |
| `src/grid.rs`, `src/terminal.rs` | Modify | `snapshot(term, &theme)`; `TerminalCore` keeps its `Theme` | 3 |
| `src/fonts.rs` | Create | `FontSettings`, `from_entries`, `parse_feature`, `gpui_font`, `register_symbols_font` | 4 |
| `assets/fonts/SymbolsNerdFontMono-Regular.ttf`, `assets/fonts/LICENSE-NerdFonts.txt` | Create | Bundled icon font + MIT licence | 4 |
| `src/appearance.rs` | Create | `Appearance` (GPUI `Global`) + `load(cx)` (the only I/O glue) | 5 |
| `src/main.rs`, `src/terminal_view.rs`, `src/workspace.rs` | Modify | Load at startup; font/size/colors in the view; sidebar from `UiColors` | 3, 5 |

---

### Task 1: `ghostty_config.rs` — read Ghostty's files — ⬜

**Files:** Create `src/ghostty_config.rs` · Modify `src/main.rs` (`mod ghostty_config;`).

**Interfaces:** Produces:
- `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Entry { pub key: String, pub value: String }`
- `pub fn parse(text: &str) -> Vec<Entry>`: one entry per `key = value` line, in order. Rules:
  - skip blank lines and lines whose first non-space character is `#`;
  - split on the first `=`; a line with no `=` is skipped;
  - trim the key and the value;
  - if the value is wrapped in one pair of double quotes, drop them;
  - an empty value is kept (it means "reset to default");
  - no inline comments: `a = b # c` gives the value `b # c`, as in Ghostty.
- `pub fn config_paths(home: &Path, xdg_config_home: Option<&Path>) -> Vec<PathBuf>`, lowest precedence first:
  1. `{xdg or home/.config}/ghostty/config`
  2. `{…}/ghostty/config.ghostty`
  3. `{home}/Library/Application Support/com.mitchellh.ghostty/config`
  4. `{…}/config.ghostty`
- `pub fn load(paths: &[PathBuf], read: &dyn Fn(&Path) -> Option<String>) -> Vec<Entry>`: the entries of each existing file, in order. `read` returns `None` for a missing file. Each file's `config-file` entries are expanded **after** that file's own entries, in the order written:
  - a path is relative to the including file's directory;
  - a leading `?` means optional (a missing optional file is silent; a missing required one logs);
  - an include is followed only once per run (cycle guard by path);
  - the `config-file` entries themselves are not returned.

- [ ] **Step 1: Failing tests** in `src/ghostty_config.rs`:

```rust
fn e(k: &str, v: &str) -> Entry { Entry { key: k.into(), value: v.into() } }

#[test]
fn parse_handles_comments_quotes_spacing_and_resets() {
    let text = "# comment\n\nfont-family = \"MesloLGS NF\"\nfont-size=14\n  background =  #112233  \nfont-family =\nnot a setting\nforeground = abc # not a comment\n";
    assert_eq!(parse(text), vec![
        e("font-family", "MesloLGS NF"), e("font-size", "14"), e("background", "#112233"),
        e("font-family", ""), e("foreground", "abc # not a comment"),
    ]);
}

#[test]
fn config_paths_follow_ghostty_order() {
    let home = Path::new("/h");
    assert_eq!(config_paths(home, None), vec![
        PathBuf::from("/h/.config/ghostty/config"),
        PathBuf::from("/h/.config/ghostty/config.ghostty"),
        PathBuf::from("/h/Library/Application Support/com.mitchellh.ghostty/config"),
        PathBuf::from("/h/Library/Application Support/com.mitchellh.ghostty/config.ghostty"),
    ]);
    assert_eq!(config_paths(home, Some(Path::new("/x")))[1], PathBuf::from("/x/ghostty/config.ghostty"));
}

#[test]
fn later_files_win_and_includes_follow_their_file() {
    let files: HashMap<&str, &str> = HashMap::from([
        ("/c/a", "font-size = 10\nconfig-file = inc\nfont-size = 11"),
        ("/c/inc", "font-size = 12"),
        ("/c/b", "font-size = 13\nconfig-file = ?missing"),
    ]);
    let read = |p: &Path| files.get(p.to_str().unwrap()).map(|s| s.to_string());
    let got = load(&[PathBuf::from("/c/a"), PathBuf::from("/nope"), PathBuf::from("/c/b")], &read);
    let sizes: Vec<&str> = got.iter().filter(|x| x.key == "font-size").map(|x| x.value.as_str()).collect();
    assert_eq!(sizes, ["10", "11", "12", "13"]); // the include comes after its file's own lines
    assert!(got.iter().all(|x| x.key != "config-file"));
}

#[test]
fn include_cycles_stop() {
    let files: HashMap<&str, &str> = HashMap::from([("/c/a", "x = 1\nconfig-file = b"), ("/c/b", "y = 2\nconfig-file = a")]);
    let read = |p: &Path| files.get(p.to_str().unwrap()).map(|s| s.to_string());
    assert_eq!(load(&[PathBuf::from("/c/a")], &read), vec![e("x", "1"), e("y", "2")]);
}
```

- [ ] **Step 2: Run** `cargo test ghostty_config`. Expected: compile errors (items not defined).
- [ ] **Step 3: Implement** as specified (`std::collections::HashSet<PathBuf>` for the cycle guard; join relative include paths onto the including file's parent).
- [ ] **Step 4: Run** `cargo test`. Expected: `test result: ok. 89 passed` (dead-code warnings until Task 5 are allowed).
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m6): read Ghostty config files in Ghostty's order, with includes"`

### Task 2: `theme.rs` — Ghostty colors and themes — ⬜

**Files:** Create `src/theme.rs` · Modify `src/main.rs` (`mod theme;`).

**Interfaces:**
- **Consumes** `ghostty_config::{Entry, parse}`.
- **`Theme`:** `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Theme` with these fields:
  - `pub background: Rgb`, `pub foreground: Rgb`;
  - `pub cursor: Option<Rgb>`, `pub cursor_text: Option<Rgb>`;
  - `pub selection_background: Option<Rgb>`, `pub selection_foreground: Option<Rgb>`;
  - `pub palette: [Rgb; 16]`.

  `Rgb` is `alacritty_terminal::vte::ansi::Rgb`. `impl Default for Theme` gives the Ghostty defaults from Global Constraints, with every `Option` set to `None`.
- **`pub fn parse_color(s: &str) -> Option<Rgb>`:** accepts `#rrggbb` or `rrggbb`, case-insensitive. Anything else gives `None`, including X11 names and 3-digit hex.
- **`pub fn apply(theme: &mut Theme, entries: &[Entry])`:** applies `background`, `foreground`, `cursor-color`, `cursor-text`, `selection-background`, `selection-foreground` and `palette`, in order.
  - An empty value resets that key to its default.
  - `cell-foreground` or `cell-background` sets an `Option` field to `None`.
  - `palette = N=COLOR`: `N` is decimal or `0x`/`0o`/`0b`-prefixed. Only 0–15 are stored; other indexes are skipped.
  - A bad value is skipped (logged) and leaves the previous value.
- **`pub fn resolve(entries: &[Entry], dark: bool, read_theme: &dyn Fn(&str) -> Option<String>) -> Theme`:**
  1. Start from `Theme::default()`.
  2. Take the last `theme` entry, if any. For `light:A,dark:B` (either order, items trimmed), pick by `dark`.
  3. Apply the theme file's entries, read with `read_theme(name)` and `parse`. Skip the keys `theme` and `config-file` inside it. A missing theme is logged and skipped.
  4. Apply the user's own `entries` on top.
- **`pub fn theme_dirs(home: &Path, xdg_config_home: Option<&Path>, resources_dir: Option<&Path>) -> Vec<PathBuf>`**, in lookup order:
  1. `{xdg or home/.config}/ghostty/themes`;
  2. `{resources_dir}/themes`, when given (`$GHOSTTY_RESOURCES_DIR`);
  3. `/Applications/Ghostty.app/Contents/Resources/ghostty/themes`;
  4. `{home}/Applications/Ghostty.app/Contents/Resources/ghostty/themes`.

  An absolute theme name is read as a path. A name containing `/` that isn't absolute is rejected.

- [ ] **Step 1: Failing tests** in `src/theme.rs`:

```rust
const MOCHA: &str = "palette = 1=#f38ba8\nbackground = #1e1e2e\nforeground = #cdd6f4\ncursor-color = #f5e0dc\nselection-background = #585b70\ntheme = Nested\nconfig-file = evil";
fn entries(text: &str) -> Vec<Entry> { crate::ghostty_config::parse(text) }
fn read(name: &str) -> Option<String> { (name == "Catppuccin Mocha").then(|| MOCHA.to_string()) }

#[test]
fn defaults_are_ghosttys() {
    let t = Theme::default();
    assert_eq!((t.background, t.foreground), (parse_color("#282c34").unwrap(), parse_color("ffffff").unwrap()));
    assert_eq!(t.palette[1], parse_color("cc6666").unwrap());
    assert_eq!(t.palette[15], parse_color("eaeaea").unwrap());
}

#[test]
fn a_named_theme_loads_and_user_values_override_it() {
    let t = resolve(&entries("theme = Catppuccin Mocha\nbackground = #000000"), true, &read);
    assert_eq!(t.background, parse_color("000000").unwrap()); // the user's own value wins
    assert_eq!(t.foreground, parse_color("cdd6f4").unwrap()); // the theme fills in the rest
    assert_eq!(t.palette[1], parse_color("f38ba8").unwrap());
    assert_eq!(t.cursor, parse_color("f5e0dc"));
}

#[test]
fn light_dark_pair_picks_by_appearance() {
    let e = entries("theme = light:Nope,dark:Catppuccin Mocha");
    assert_eq!(resolve(&e, true, &read).background, parse_color("1e1e2e").unwrap());
    assert_eq!(resolve(&e, false, &read), Theme::default()); // "Nope" missing → defaults
}

#[test]
fn bad_values_are_skipped() {
    let t = resolve(&entries("background = navy\nforeground = #12345\npalette = 300=#ffffff\npalette = x=#ffffff\npalette = 0x2=#010203\ncursor-color = cell-foreground"), true, &read);
    assert_eq!((t.background, t.foreground), (Theme::default().background, Theme::default().foreground));
    assert_eq!(t.palette[2], parse_color("010203").unwrap());
    assert_eq!(t.cursor, None);
}

#[test]
fn empty_value_resets_and_missing_theme_keeps_defaults() {
    let t = resolve(&entries("background = #000000\nbackground =\ntheme = Nope"), true, &read);
    assert_eq!(t, Theme::default());
}

#[test]
fn theme_dirs_search_user_then_ghostty_app() {
    let d = theme_dirs(Path::new("/h"), None, Some(Path::new("/r")));
    assert_eq!(d[0], PathBuf::from("/h/.config/ghostty/themes"));
    assert_eq!(d[1], PathBuf::from("/r/themes"));
    assert_eq!(d[2], PathBuf::from("/Applications/Ghostty.app/Contents/Resources/ghostty/themes"));
}
```

- [ ] **Step 2: Run** `cargo test theme`. Expected: compile errors.
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run** `cargo test`. Expected: `test result: ok. 95 passed`.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m6): Ghostty theme and colors with Ghostty's defaults and precedence"`

### Task 3: Terminal and window colors come from the theme — ⬜

**Files:** Modify `src/colors.rs`, `src/grid.rs`, `src/terminal.rs`, `src/terminal_view.rs`, `src/workspace.rs`, `src/theme.rs`.

**Interfaces:**
- **Removed constants:** `colors::FOREGROUND`, `BACKGROUND` and `SELECTION` are gone. These now take `theme: &Theme`:
  - `colors::resolve(color, overrides, theme)`
  - `colors::resolve_index(index, overrides, theme)`
  - `colors::indexed_rgb(i, theme)`

  Named colors `Foreground`/`Background` resolve to `theme.foreground`/`theme.background`. Indexes 0–15 come from `theme.palette`. A program's OSC overrides still win.
- **`grid::snapshot(term, theme: &Theme)`:**
  - The default background check uses `theme.background`.
  - Selected cells get `theme.selection_background.unwrap_or(mix(theme.background, theme.foreground, 0.25))`.
  - When `selection_foreground` is set, selected cells use it as their foreground too.
- **`TerminalCore::spawn(cmd, dims, events, theme: Theme)`:** the core stores the theme. `snapshot()` and `color()` use it, and a public `theme()` getter returns it.
- **Cursor:** the view's block cursor uses `theme.cursor.unwrap_or(fg)` as its fill and `theme.cursor_text.unwrap_or(bg)` for the glyph.
- **In `theme.rs`:**
  - `pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb`: linear per channel, `t = 0` gives `a`.
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct UiColors { pub sidebar_background, pub sidebar_border, pub row_active, pub row_hover, pub text, pub text_dim, pub ring_idle, pub ring_working, pub ring_waiting, pub ring_done }`, all `Rgb`.
  - `impl Theme { pub fn ui(&self) -> UiColors }`, with these values:

    | Field | Value |
    |---|---|
    | `sidebar_background` | `mix(bg, black, 0.18)` when the background is dark, `mix(bg, black, 0.06)` when light |
    | `sidebar_border` | `mix(bg, fg, 0.12)` |
    | `row_active` | `mix(bg, fg, 0.12)` |
    | `row_hover` | `mix(bg, fg, 0.06)` |
    | `text` | `fg` |
    | `text_dim` | `mix(bg, fg, 0.6)` |
    | `ring_idle` | `text_dim` |
    | `ring_working` | `palette[3]` |
    | `ring_waiting` | `palette[1]` |
    | `ring_done` | `palette[2]` |

    "Dark" means a relative luminance below 0.5.
- **The workspace sidebar** takes every color from `theme.ui()`. In this task it uses `Theme::default().ui()`; Task 5 switches it to the loaded theme. No `rgb(0x…)` literals remain in `workspace.rs`.

- [ ] **Step 1: Failing tests:**
  1. Update every existing `grid.rs` and `colors.rs` test to pass `&Theme::default()`, and replace `colors::FOREGROUND`/`BACKGROUND`/`SELECTION` with the theme's values.
  2. Update `selected_cells_get_the_selection_background` to expect `mix(bg, fg, 0.25)` for the default theme.
  3. Add to `colors.rs`: `named_colors_and_palette_follow_the_theme`. A `Theme` with `background = #000000` and `palette[1] = #ff0000` gives `resolve(Named(Background))` = `#000000` and `indexed_rgb(1)` = `#ff0000`. `indexed_rgb(196)` stays `#ff0000` and `indexed_rgb(232)` stays `#080808`.
  4. Add to `theme.rs`: `ui_colors_follow_light_and_dark_themes`. For `Theme::default()`, `sidebar_background` is darker than `background` (lower luminance), and `ring_waiting == palette[1]`. For a light theme (`background = #ffffff`, `foreground = #000000`), `text_dim` is not equal to `background`, and `row_active` lies strictly between them in luminance.
- [ ] **Step 2: Run** `cargo test`. Expected: compile errors (signatures changed, items missing).
- [ ] **Step 3: Implement** as specified. `TerminalView::spawn` passes `Theme::default()` until Task 5.
- [ ] **Step 4: Run** `cargo test`. Expected: `test result: ok. 97 passed`. Run `cargo build`: expect no warnings except allowed dead code.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m6): terminal colors, cursor, selection and sidebar come from the theme"`

### Task 4: `fonts.rs` — family, size, features, and the bundled icon font — ⬜

**Files:**
- Create `src/fonts.rs`.
- Create `assets/fonts/SymbolsNerdFontMono-Regular.ttf` and `assets/fonts/LICENSE-NerdFonts.txt` (the MIT licence text). Both come from the Nerd Fonts release `NerdFontsSymbolsOnly.zip` (`https://github.com/ryanoasis/nerd-fonts/releases/latest/download/NerdFontsSymbolsOnly.zip`).
- Modify `src/main.rs` (`mod fonts;`).

**Interfaces:**
- **Consumes:** `ghostty_config::Entry`.
- **`FontSettings`:** `#[derive(Clone, Debug, PartialEq)] pub struct FontSettings { pub families: Vec<String>, pub size: f32, pub features: Vec<(String, u32)> }`.
  - `pub const DEFAULT_SIZE: f32 = 13.0`.
  - `pub const SYMBOLS_FAMILY: &str = "Symbols Nerd Font Mono"`.
- **`pub fn from_entries(entries: &[Entry]) -> FontSettings`:**
  - `font-family` appends to `families`; an empty value clears the list.
  - `font-size` parses an `f32` greater than 0 (the last valid one wins; an empty value resets to 13).
  - `font-feature` appends each comma-separated item through `parse_feature`; an empty value clears the list.
- **`pub fn parse_feature(s: &str) -> Option<(String, u32)>`:**
  - The tag may be quoted and must be exactly 4 ASCII alphanumerics.
  - `tag`, `+tag`, `tag on` and `tag=1` give 1.
  - `-tag`, `tag off` and `tag=0` give 0.
  - `tag=N` gives N.
  - Anything else gives `None`.
- **`pub fn gpui_font(s: &FontSettings, installed: &[String]) -> gpui::Font`:**
  - Keep only the families that appear in `installed`.
  - The primary font is the first one kept. If none are kept, use `"JetBrains Mono"` when installed, else `"Menlo"`.
  - `fallbacks` are the rest of the kept families, then `SYMBOLS_FAMILY`.
  - `features` is `FontFeatures(Arc::new(s.features.clone()))`.
- **`pub fn register_symbols_font() -> bool`:**
  1. Write the bundled bytes (`include_bytes!("../assets/fonts/SymbolsNerdFontMono-Regular.ttf")`) to `~/Library/Caches/fuxx/SymbolsNerdFontMono-Regular.ttf` when that file is missing or a different size.
  2. Register it with `CTFontManagerRegisterFontsForURL(url, kCTFontManagerScopeProcess = 1, null)`, using a `CFURLCreateFromFileSystemRepresentation` URL. Declare both in an `unsafe extern "C"` block with `#[link(name = "CoreText", kind = "framework")]` and CoreFoundation, then `CFRelease` the URL.
  3. Return whether registration succeeded. Already being registered counts as success, so a second call in the same process returns `true`.

  This is the code the 2026-10-27 spike proved.

- [ ] **Step 1: Failing tests** in `src/fonts.rs`:

```rust
fn e(k: &str, v: &str) -> Entry { Entry { key: k.into(), value: v.into() } }

#[test]
fn settings_accumulate_reset_and_default() {
    let s = from_entries(&[e("font-family", "A"), e("font-family", "B"), e("font-size", "x"), e("font-size", "14.5"), e("font-feature", "-calt, liga=0")]);
    assert_eq!(s, FontSettings { families: vec!["A".into(), "B".into()], size: 14.5, features: vec![("calt".into(), 0), ("liga".into(), 0)] });
    let r = from_entries(&[e("font-family", "A"), e("font-family", ""), e("font-size", "")]);
    assert_eq!((r.families.len(), r.size), (0, DEFAULT_SIZE));
}

#[test]
fn parse_feature_forms() {
    for (s, want) in [("calt", Some(1)), ("+calt", Some(1)), ("calt on", Some(1)), ("calt=1", Some(1)),
                      ("-calt", Some(0)), ("calt off", Some(0)), ("calt=0", Some(0)), ("\"ss01\"=2", Some(2)),
                      ("toolong", None), ("ca", None), ("calt=x", None)] {
        assert_eq!(parse_feature(s).map(|(_, v)| v), want, "{s}");
    }
}

#[test]
fn gpui_font_skips_missing_families() {
    let installed = vec!["Menlo".to_string(), "MesloLGS NF".to_string()];
    let f = gpui_font(&FontSettings { families: vec!["Nope".into(), "MesloLGS NF".into(), "Menlo".into()], size: 13.0, features: vec![] }, &installed);
    assert_eq!(f.family.as_ref(), "MesloLGS NF");
    assert_eq!(f.fallbacks.unwrap().fallback_list(), ["Menlo".to_string(), SYMBOLS_FAMILY.to_string()]);
    let d = gpui_font(&FontSettings { families: vec![], size: 13.0, features: vec![] }, &installed);
    assert_eq!(d.family.as_ref(), "Menlo"); // JetBrains Mono not installed here
}

#[test]
fn bundled_symbols_draw_the_apple_icon() {
    // Headless macOS text system (no window). Proves the icon comes from a real font, not
    // CoreText's last-resort "?" box, once the bundled font is registered.
    use gpui::{FontRun, px};
    assert!(register_symbols_font());
    let ts = gpui_platform::current_platform(true).text_system();
    let font = gpui_font(&FontSettings { families: vec!["Menlo".into()], size: 13.0, features: vec![] }, &ts.all_font_names());
    let id = ts.font_id(&font).unwrap();
    let text = "a\u{f179}b";
    let layout = ts.layout_line(text, px(13.0), &[FontRun { len: text.len(), font_id: id }]);
    let icon_run = layout.runs.iter().find(|r| r.font_id != id).expect("icon drawn by a fallback font");
    let plain = ts.font_id(&gpui::font("Menlo")).unwrap();
    let last_resort = ts.layout_line(text, px(13.0), &[FontRun { len: text.len(), font_id: plain }]);
    let lr = last_resort.runs.iter().find(|r| r.font_id != plain).map(|r| r.font_id);
    assert_ne!(Some(icon_run.font_id), lr, "icon must not come from the last-resort font");
}
```

- [ ] **Step 2: Run** `cargo test fonts`. Expected: compile errors.
- [ ] **Step 3: Add the assets.** Unzip the font and licence into `assets/fonts/`, then implement as specified.
- [ ] **Step 4: Run** `cargo test`. Expected: `test result: ok. 101 passed`. If `bundled_symbols_draw_the_apple_icon` can't create the headless platform in `cargo test`, report it. Don't drop the test: the controller rules on it.
- [ ] **Step 5: Commit:** `git add assets src && git commit -m "feat(m6): font settings from the config and the bundled Symbols Nerd Font"`

### Task 5: Load at startup and wire the view — ⬜

**Files:** Create `src/appearance.rs` · Modify `src/main.rs`, `src/terminal_view.rs`, `src/workspace.rs`.

**Interfaces:**
- **`Appearance`:** `pub struct Appearance { pub theme: Theme, pub font: gpui::Font, pub font_size: f32 }` with `impl gpui::Global for Appearance {}`.
- **`pub fn load(cx: &App) -> Appearance`** is the only place that touches the real filesystem and environment:
  - `home` is `$HOME`; `xdg` is `$XDG_CONFIG_HOME`; the resources dir is `$GHOSTTY_RESOURCES_DIR`.
  - Entries come from `ghostty_config::load(&config_paths(..), &|p| std::fs::read_to_string(p).ok())`.
  - Theme names are read from the first `theme_dirs(..)` entry that has the file. An absolute name is read as a path.
  - `dark` is true unless `cx.window_appearance()` is `Light` or `VibrantLight`.
  - The font comes from `gpui_font(&from_entries(..), &cx.text_system().all_font_names())`, after `register_symbols_font()` has run. A `false` result is logged.
  - Log one line: `[fuxx] config: font <family> <size>pt, theme <name or default>`.
- **`main.rs`:** call `cx.set_global(appearance::load(cx))` before opening the window.
- **`TerminalView`:** reads `cx.global::<Appearance>()`:
  - `CellMetrics` uses `font` and `font_size` instead of `FONT_FAMILY`/`FONT_SIZE`, which are removed;
  - the paint background and cursor use the core's theme;
  - `TerminalView::spawn` passes `theme` to `TerminalCore::spawn`.
- **`Workspace`:** gets its `UiColors` from `cx.global::<Appearance>().theme.ui()`.

> **No new unit tests:** this is startup glue and GPUI wiring. Every decision is in the tested
> functions of Tasks 1–4. Verified by a clean build here and by you in Task 6.

- [ ] **Step 1: Implement** as specified.
- [ ] **Step 2: Build with zero warnings:** `cargo build 2>&1 | grep -E "^(warning|error)" | grep -v "future version"`. Expected: **no output**. `cargo test` gives `101 passed`. `cargo fmt --check` is clean.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m6): load Ghostty config at startup; font, size and theme in the view and sidebar"`

### Task 6: On screen — 👀 ⬜

Run `cargo run` from Ghostty and report.

- [ ] The log shows `[fuxx] config: font MesloLGS NF 13pt, theme default`.
- [ ] Your powerlevel10k prompt shows its **icons** (the Apple logo, folder, clock), not `?` boxes.
- [ ] The colors match Ghostty: same background `#282c34`, same `ls --color` colors side by side. The sidebar is a darker shade of the same background.
- [ ] Theme switch: add `theme = Catppuccin Mocha` to `~/Library/Application Support/com.mitchellh.ghostty/config.ghostty` and restart fuxx. fuxx turns Catppuccin, sidebar and rings included. Then try a light theme, `theme = Catppuccin Latte`: is the text readable, and is the selection visible? **Remove the line afterwards.**
- [ ] Font fallback for everyone: temporarily set `font-family = Menlo`. The prompt icons still show (bundled font). Remove it afterwards.
- [ ] Broken config: temporarily add `background = notacolor` and `config-file = /nope`. fuxx still starts with default colors. Remove them afterwards.
- [ ] Ligatures (only if you have a ligature font installed, e.g. JetBrains Mono or Fira Code): set it as `font-family`. `=> != >=` render joined, and text after them stays aligned. `font-feature = -calt` turns them off. Otherwise mark "not tested".
- [ ] M5 behaviours unchanged: selection is visible, cmd-click links work, rings change.

### Task 7: Update the docs — ⬜

- [ ] [[46 - Milestone 6 - Fonts and Config]]: tick the checklist; **Status:: ✅ Done**.
- [ ] [[40 - Roadmap]]: M6 → ✅ (link the note).
- [ ] [[00 - Plan Tracker (Start Here)]]: M6 row → ✅ 7/7; **Where we are** → M7, plan to be written. Add an M6 row to the "How these plans were made" table.
- [ ] [[31 - API Fact Sheet (pinned rev)]]: the `Font` / `FontFallbacks` / `FontFeatures` facts, "in-memory fonts are ignored by CoreText's fallback → register the file", and Ghostty's config order/defaults.
- [ ] [[30 - Tech Stack]]: the bundled Symbols Nerd Font (MIT) and the CoreText registration.
- [ ] `STRUCTURE.md`: `ghostty_config.rs`, `theme.rs`, `fonts.rs`, `appearance.rs`, `assets/fonts/`.
- [ ] [[60 - Progress Log]]: entry "M6 complete: fonts and config".
- [ ] Commit docs.

## Verification log

| Date | Check | Result | Evidence |
|---|---|---|---|
| 2026-10-27 | Ghostty 1.3.1 config facts: file order, syntax, includes, theme lookup and precedence, defaults | ✅ | Ghostty's bundled docs + `+show-config --default` + empirical tests with temp XDG dirs (research agent) |
| 2026-10-27 | This Mac: your effective Ghostty config is `font-family = "MesloLGS NF"` (App Support `config.ghostty`) + `shell-integration = none` | ✅ | `ghostty +show-config`, file reads |
| 2026-10-27 | GPUI fonts at the pinned rev: `Font { fallbacks, features }`, CoreText cascade = user fallbacks then system; `resolve_font` silently falls back to Helvetica for a missing family | ✅ | Research agent, file:line refs |
| 2026-10-27 | Spike: icon U+F179 with Menlo → last-resort `?` font; with installed MesloLGS NF fallback → real glyph; with the font only in memory (`add_fonts`) → still `?`; after `CTFontManagerRegisterFontsForURL` (process scope) → real glyph from the bundled font | ✅ | AI headless spike (`gpui_platform::current_platform(true)`) |
| — | Task code itself | ⚠️ not pre-compiled (signatures and tests) | Verified per task during execution |

## Known limitations

| Limitation | Lands in |
|---|---|
| Config is read once at startup (restart to apply) | Polish, if wanted |
| X11 color names, `bold-color`, `minimum-contrast`, `palette-generate`, `adjust-*`, padding/opacity are ignored | Later, if wanted |
| `font-family-bold/italic` ignored (bold/italic faces of the main family are used) | Later, if wanted |
| Ligature fonts that merge `=>` into one wide glyph would misalign the rest of the line (GPUI locks glyphs to cells); contextual-alternates fonts (JetBrains Mono, Fira Code) are fine | Revisit if a font misbehaves |
| Default font is JetBrains Mono only if installed (Ghostty bundles it; fuxx doesn't) | Later, if wanted |
| Tab doesn't accept Claude Code's grey suggestion | Tech debt in [[00 - Plan Tracker (Start Here)]] |

## Related

[[00 - Plan Tracker (Start Here)]] · [[M5 Plan - Terminal UX]] · [[40 - Roadmap]] · [[31 - API Fact Sheet (pinned rev)]] · [[12 - Scope]]
