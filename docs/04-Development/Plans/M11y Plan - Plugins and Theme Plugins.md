# M11y — Plugins and Theme Plugins Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Install a theme plugin by pasting a public GitHub link in Settings → Plugins, use its themes in Appearance, update it, and uninstall it so that nothing of it is left.

**Architecture:** Four pure-ish modules (`plugin_manifest`, `plugin_source`, `plugin_store`, `plugin_themes`) hold every rule and are unit-tested in temp folders; `plugin_fetch` is the only code that runs `curl`, `tar` and `ditto`, behind a `Net` trait so tests use local files; `plugins` is the GPUI global that runs install / update / uninstall off the UI thread and asks for confirmation; Preferences gets a Plugins page and plugin themes join the theme list.

**Tech Stack:** Rust, GPUI (pinned rev), `serde` + `serde_json` (already in), system `/usr/bin/curl`, `/usr/bin/tar`, `/usr/bin/ditto`, `/usr/bin/shasum`. **No new crate.**

**Spec:** [[76 - Milestones 11y-11z - Plugins]] (`docs/04-Development/76 - Milestones 11y-11z - Plugins.md`), §1–§8, §11, §12 and the M11y row. M11z (§9, §10) is a separate plan.

**Branch:** `m11y-plugins` from `master`. No commits until you say so (AGENTS.md); each task ends with a checkpoint (tests green) instead.

## Global Constraints

- Manifest file name `fuxx-plugin.conf`, parsed with `settings::parse` (repeated keys kept in order). `format = 1`.
- `id`: `a-z`, `0-9`, `-`, 2–40 characters.
- Required keys: `format`, `id`, `name`, `version`. Optional: `description`, `theme` (repeatable). Unknown keys ignored.
- **M11y refuses plugins with `bin` or `zsh`** (M11z lifts this): "This plugin runs a program. fuxx can install those from a later version."
- Links accepted: `https://github.com/owner/repo`, `http://…`, `github.com/owner/repo`, `www.github.com/owner/repo`, `owner/repo`; trailing `.git` or `/` and surrounding spaces ignored. Anything else: "Paste a GitHub repo link, like github.com/owner/repo."
- Source: latest release (asset `fuxx-plugin-macos-<arm64|x86_64>.zip`, else `fuxx-plugin-macos.zip`, else the release's `tarball_url`); no release (GitHub 404) → `https://api.github.com/repos/<o>/<r>/commits/HEAD` and its tarball.
- Limits: 100 MB (`104857600` bytes) per download, 60 s per request.
- Folders: `~/Library/Application Support/fuxx/plugins/` (`plugins.json`, `<id>/`, `.staging/`) and `~/Library/Application Support/fuxx/plugin-data/<id>/`.
- **Uninstall deletes only `plugins/<id>/` and `plugin-data/<id>/`**, plus a `theme =` line in the main config naming one of its themes (plain or either half of `light:A,dark:B` → the whole `theme` key removed). Never an included `config-file`.
- Updates: checked 15 s after launch and when the Plugins page opens. Never installed without a click.
- Plugin themes join the one sorted theme list; a name already taken (ignoring case) becomes `<name> (<plugin name>)`.
- Copy is fixed by this plan (quoted strings below); keep it exactly.

## Review Focus

- **A repo that isn't a plugin** (someone pastes any repo): "This repo isn't a fuxx plugin (it has no fuxx-plugin.conf)." Test in Task 4.
- **Near-miss links** (`github.com/o/r/tree/main`, `git@github.com:o/r.git`, a gist link): the hint above, nothing downloaded. Test in Task 2.
- **GitHub's rate limit** (403 or 429): "GitHub is limiting requests from this Mac for now. Try again in an hour." not "GitHub answered 403". Test in Task 4.
- **fuxx quits mid-install or mid-uninstall**: next launch leaves no half plugin, no stray record, no staging files. Test in Task 3.
- **Install clicked twice, or Uninstall while an install runs**: one operation at a time; the second click does nothing. Test in Task 6.

---

### Task 1: The manifest (`plugin_manifest.rs`)

**Files:**
- Create: `src/plugin_manifest.rs`
- Modify: `src/main.rs` (add `mod plugin_manifest;`)

**Interfaces:**
- Consumes: `settings::parse(&str) -> Vec<Entry>`
- Produces:
  ```rust
  pub const MANIFEST: &str = "fuxx-plugin.conf";
  pub const FORMAT: u32 = 1;
  #[derive(Clone, Debug, PartialEq)]
  pub struct Manifest { pub format: u32, pub id: String, pub name: String, pub version: String,
      pub description: Option<String>, pub themes: Vec<String>, pub bin: Option<String>, pub zsh: Vec<String> }
  impl Manifest { pub fn runs_code(&self) -> bool; pub fn theme_names(&self) -> Vec<String> }
  #[derive(Clone, Debug, PartialEq)]
  pub enum ManifestError { Missing(&'static str), BadId(String), BadFormat(String), NewerFormat(u32),
      PathOutside(String), FileMissing(String), RunsCode }
  impl ManifestError { pub fn message(&self) -> String }
  pub fn valid_id(id: &str) -> bool;
  pub fn parse(text: &str) -> Result<Manifest, ManifestError>;
  pub fn check_files(root: &Path, m: &Manifest) -> Result<(), ManifestError>;
  pub fn accept(m: &Manifest, code_allowed: bool) -> Result<(), ManifestError>;
  pub const CODE_ALLOWED: bool = false; // M11z sets true
  ```

- [ ] **Step 1: Write the failing tests** (in the module's `#[cfg(test)] mod tests`)

```rust
#[test] fn parses_the_theme_example() {
    let m = parse("format = 1\nid = sunset-themes\nname = Sunset Themes\nversion = 1.0.0\n\
                   description = Warm themes.\ntheme = themes/Sunset\ntheme = themes/Sunset Light\n").unwrap();
    assert_eq!(m.id, "sunset-themes");
    assert_eq!(m.themes, ["themes/Sunset", "themes/Sunset Light"]);
    assert_eq!(m.theme_names(), ["Sunset", "Sunset Light"]);
    assert!(!m.runs_code());
}
#[test] fn refusals() {
    assert_eq!(parse("id = a1\nname = n\nversion = 1"), Err(ManifestError::Missing("format")));
    assert_eq!(parse("format = 2\nid = ab\nname = n\nversion = 1"), Err(ManifestError::NewerFormat(2)));
    assert_eq!(parse("format = one\nid = ab\nname = n\nversion = 1"), Err(ManifestError::BadFormat("one".into())));
    assert_eq!(parse("format = 1\nid = A_b\nname = n\nversion = 1"), Err(ManifestError::BadId("A_b".into())));
}
#[test] fn ids() {
    assert!(valid_id("ab")); assert!(valid_id("fuxx-completes")); assert!(valid_id(&"a".repeat(40)));
    assert!(!valid_id("a")); assert!(!valid_id(&"a".repeat(41))); assert!(!valid_id("Ab"));
    assert!(!valid_id("a/b")); assert!(!valid_id(".."));
}
#[test] fn unknown_keys_are_ignored() { assert!(parse("format = 1\nid = ab\nname = n\nversion = 1\nscreen = x").is_ok()); }
#[test] fn code_is_refused_in_m11y() {
    let m = parse("format = 1\nid = ab\nname = n\nversion = 1\nzsh = a.zsh").unwrap();
    assert!(m.runs_code());
    assert_eq!(accept(&m, CODE_ALLOWED), Err(ManifestError::RunsCode));
}
#[test] fn files_must_exist_inside_the_folder() {
    // temp dir with themes/Sunset; a manifest naming "../x", "/etc/hosts", "themes/Nope",
    // and a symlink themes/Link -> outside file: PathOutside / PathOutside / FileMissing / PathOutside
}
```

- [ ] **Step 2: Run** `cargo test --bin fuxx plugin_manifest` — Expected: FAIL (module empty).

- [ ] **Step 3: Implement.** `parse` reads the last value for single keys and every value for `theme`/`zsh`, checks in the order `format` (missing → `Missing("format")`, not a `u32` → `BadFormat`, `> FORMAT` → `NewerFormat`), `id`, `name`, `version`. `check_files`: each `theme` path must be relative, have no `..` component, `symlink_metadata` must be a regular file (a symlink → `PathOutside`), missing → `FileMissing`. `bin` must be a directory, `zsh` files regular files (same rules; checked even though M11y refuses code, so M11z reuses it). Copy for `message()`:
  - `Missing(k)`: "This plugin's fuxx-plugin.conf has no `{k}`."
  - `BadId(id)`: "This plugin's id \"{id}\" isn't valid (use a-z, 0-9 and -, 2 to 40 characters)."
  - `BadFormat(s)`: "This plugin's format \"{s}\" isn't a number."
  - `NewerFormat(n)`: "This plugin needs a newer fuxx (plugin format {n})."
  - `PathOutside(p)`: "This plugin names a file outside its folder: {p}"
  - `FileMissing(p)`: "This plugin names a file it doesn't have: {p}"
  - `RunsCode`: "This plugin runs a program. fuxx can install those from a later version."

- [ ] **Step 4: Run** `cargo test --bin fuxx plugin_manifest` — Expected: PASS.

- [ ] **Step 5: Checkpoint** `cargo test --bin fuxx && cargo clippy --all-targets 2>&1 | grep plugin_manifest` — Expected: all pass, no clippy lines.

---

### Task 2: Links, GitHub answers, what to download (`plugin_source.rs`)

**Files:**
- Create: `src/plugin_source.rs`; Modify: `src/main.rs` (`mod plugin_source;`)

**Interfaces:**
- Produces:
  ```rust
  pub const LINK_HINT: &str = "Paste a GitHub repo link, like github.com/owner/repo.";
  #[derive(Clone, Debug, PartialEq, Eq)] pub struct Repo { pub owner: String, pub name: String }
  impl Repo { pub fn slug(&self) -> String /* "owner/name" */; pub fn page(&self) -> String /* "https://github.com/owner/name" */;
      pub fn release_api(&self) -> String /* …/repos/o/r/releases/latest */; pub fn head_api(&self) -> String /* …/repos/o/r/commits/HEAD */ }
  pub fn parse_link(text: &str) -> Option<Repo>;
  #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
  #[serde(rename_all = "lowercase")] pub enum Installed { Tag(String), Commit(String) }
  impl Installed { pub fn label(&self) -> String /* tag, or first 7 of the commit */ }
  #[derive(Clone, Copy, Debug, PartialEq)] pub enum ArchiveKind { Zip, Tarball }
  #[derive(Clone, Debug, PartialEq)] pub struct Download { pub url: String, pub kind: ArchiveKind, pub sha256: Option<String>, pub installed: Installed }
  #[derive(Clone, Debug, PartialEq)] pub struct ReleaseAsset { pub name: String, pub url: String, pub sha256: Option<String> }
  #[derive(Clone, Debug, PartialEq)] pub struct LatestRelease { pub tag: String, pub tarball_url: String, pub assets: Vec<ReleaseAsset> }
  pub fn parse_release(json: &str) -> Result<LatestRelease, String>;   // tag_name, tarball_url, assets[].name/browser_download_url/digest ("sha256:<hex>")
  pub fn parse_head(json: &str) -> Result<String, String>;            // "sha"
  pub fn chip() -> &'static str;                                       // "arm64" on aarch64, else "x86_64"
  pub fn from_release(r: &LatestRelease, chip: &str) -> Download;
  pub fn from_head(repo: &Repo, sha: &str) -> Download;               // https://api.github.com/repos/o/r/tarball/<sha>
  pub fn is_newer(installed: &Installed, latest: &Installed) -> bool; // any difference
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn links() {
    let r = Repo { owner: "fitzfitz".into(), name: "sunset".into() };
    for ok in ["https://github.com/fitzfitz/sunset", "http://github.com/fitzfitz/sunset/", " github.com/fitzfitz/sunset.git ",
               "www.github.com/fitzfitz/sunset", "fitzfitz/sunset"] { assert_eq!(parse_link(ok), Some(r.clone()), "{ok}"); }
    for bad in ["", "sunset", "github.com/fitzfitz", "github.com/o/r/tree/main", "git@github.com:o/r.git",
                "https://gist.github.com/o/abc", "https://gitlab.com/o/r", "o/r/x", "../x/y"] { assert_eq!(parse_link(bad), None, "{bad}"); }
}
#[test] fn picks_the_asset_for_this_chip() {
    // assets: arm64 zip, x86_64 zip, universal zip → arm64 picks arm64 (Zip, its sha256); "x86_64" picks x86_64;
    // only universal → universal; no assets → tarball_url as Tarball, sha256 None; installed = Tag(tag) in all
}
#[test] fn head_is_a_commit_tarball() {
    let d = from_head(&Repo { owner: "o".into(), name: "r".into() }, "abc1234def");
    assert_eq!(d.url, "https://api.github.com/repos/o/r/tarball/abc1234def");
    assert_eq!(d.installed, Installed::Commit("abc1234def".into()));
    assert_eq!(d.installed.label(), "abc1234");
}
#[test] fn parses_githubs_json() { /* a trimmed real releases/latest body with digest "sha256:ab…"; a commits/HEAD body with "sha" */ }
#[test] fn newer_means_different() {
    assert!(is_newer(&Installed::Tag("v1".into()), &Installed::Tag("v2".into())));
    assert!(!is_newer(&Installed::Tag("v1".into()), &Installed::Tag("v1".into())));
    assert!(is_newer(&Installed::Commit("a".into()), &Installed::Tag("v1".into())));
}
```

- [ ] **Step 2: Run** `cargo test --bin fuxx plugin_source` — Expected: FAIL.
- [ ] **Step 3: Implement** with `serde_json::Value` (as `update_release::parse_latest` does). `parse_link`: trim; strip `https://`/`http://`, then `www.`; if it starts with `github.com/` take the rest, else the text must be exactly `owner/repo`; strip one trailing `/` then `.git`; exactly two non-empty segments of `[A-Za-z0-9._-]`, neither `.` nor `..`.
- [ ] **Step 4: Run** `cargo test --bin fuxx plugin_source` — Expected: PASS.
- [ ] **Step 5: Checkpoint** as Task 1.

---

### Task 3: Folders, the record, install / replace / uninstall (`plugin_store.rs`)

**Files:**
- Create: `src/plugin_store.rs`; Modify: `src/main.rs` (`mod plugin_store;`)

**Interfaces:**
- Consumes: `Manifest`, `valid_id` (Task 1); `Repo`, `Installed` (Task 2); `settings::write_atomic`.
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq)] pub struct Folders { pub plugins: PathBuf, pub data: PathBuf }
  impl Folders { pub fn under(home: &Path) -> Folders; pub fn staging(&self) -> PathBuf; pub fn plugin(&self, id: &str) -> PathBuf; pub fn data_of(&self, id: &str) -> PathBuf }
  #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
  pub struct Record { pub id: String, pub name: String, pub version: String, pub description: Option<String>,
      pub repo: String, pub installed: Installed, pub themes: Vec<String>, pub installed_at: u64 }
  pub fn load(f: &Folders) -> Vec<Record>;                       // plugins.json {"plugins":[…]}; missing or unreadable → []
  pub fn save(f: &Folders, records: &[Record]) -> std::io::Result<()>;
  pub fn commit(f: &Folders, plugin_dir: &Path, m: &Manifest, repo: &Repo, installed: &Installed, now: u64) -> Result<Record, String>;
  pub fn uninstall(f: &Folders, id: &str) -> Result<Record, String>;
  pub fn launch_cleanup(f: &Folders);
  ```

- [ ] **Step 1: Write the failing tests** (temp `HOME`-like dir per test)

```rust
#[test] fn folders_live_in_application_support() {
    let f = Folders::under(Path::new("/h"));
    assert_eq!(f.plugins, Path::new("/h/Library/Application Support/fuxx/plugins"));
    assert_eq!(f.data, Path::new("/h/Library/Application Support/fuxx/plugin-data"));
    assert_eq!(f.staging(), f.plugins.join(".staging"));
}
#[test] fn install_then_uninstall_leaves_nothing() {
    // commit a staged folder with themes/Sunset; data_of(id) created with a file in it (as a plugin would);
    // load() has one record; uninstall(id) → plugins/<id> and plugin-data/<id> gone, load() empty,
    // and the plugins folder holds only plugins.json (and .staging if present)
}
#[test] fn uninstall_never_follows_a_symlink_out() {
    // plugins/<id>/escape -> <tmp>/outside/ (a dir with keep.txt); uninstall → keep.txt still exists
}
#[test] fn same_id_from_another_repo_is_refused() {
    // commit id "sunset" from o/a, then from o/b → Err("A plugin called sunset is already installed from github.com/o/a.")
}
#[test] fn same_id_same_repo_replaces() { /* second commit keeps one record with the new installed + version, old files gone */ }
#[test] fn uninstall_refuses_a_bad_or_unknown_id() { /* "../x" and "nope" → Err, nothing deleted */ }
#[test] fn launch_cleanup_mends_a_crash() {
    // .staging/123-x/ with files; .trash-sunset-1/; plugins/orphan/ (valid id, no record); a record "gone" with no folder
    // → .staging empty, no .trash-*, orphan deleted, record "gone" dropped and saved; a non-id folder "Notes" left alone
}
```

- [ ] **Step 2: Run** `cargo test --bin fuxx plugin_store` — Expected: FAIL.
- [ ] **Step 3: Implement.** `commit`: refuse a different repo for the same id; if `plugin(id)` exists, rename it to `plugins/.trash-<id>-<now>`; rename `plugin_dir` to `plugin(id)` (same volume: staging is inside `plugins/`); save the record; then `remove_dir_all` the trash. `uninstall`: `valid_id` first; the record must exist; rename `plugin(id)` to a trash name, save records without it, `remove_dir_all` the trash and `data_of(id)` if present (`std::fs::remove_dir_all` doesn't follow symlinks inside). `launch_cleanup`: empty `.staging/`, remove `.trash-*`, drop records with no folder, remove folders whose name is a `valid_id` with no record. Copy: unknown id → "No plugin called {id} is installed."
- [ ] **Step 4: Run** `cargo test --bin fuxx plugin_store` — Expected: PASS.
- [ ] **Step 5: Checkpoint** as Task 1.

---

### Task 4: Download, unpack, check (`plugin_fetch.rs`)

**Files:**
- Create: `src/plugin_fetch.rs`; Modify: `src/main.rs` (`mod plugin_fetch;`)

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces:
  ```rust
  pub const MAX_BYTES: u64 = 104_857_600;
  #[derive(Clone, Debug, PartialEq)] pub enum FetchError { Offline, Status(u16), TooBig }
  pub trait Net { fn get(&self, url: &str) -> Result<String, FetchError>; fn download(&self, url: &str, to: &Path) -> Result<(), FetchError>; }
  pub struct Curl;  impl Net for Curl
  pub fn latest(net: &dyn Net, repo: &Repo, chip: &str) -> Result<Download, String>;
  pub struct Staged { pub plugin_dir: PathBuf, pub work: PathBuf, pub manifest: Manifest, pub installed: Installed }
  pub fn stage(net: &dyn Net, d: &Download, staging: &Path) -> Result<Staged, String>;
  pub fn find_plugin_root(dir: &Path) -> Option<PathBuf>;
  pub fn error_text(e: &FetchError) -> String;
  ```

- [ ] **Step 1: Write the failing tests** with a `FakeNet` (a map from URL to `Result<String, FetchError>` and URL → local file to copy)

```rust
#[test] fn release_first_then_head_then_no_repo() {
    // release_api → JSON  ⇒ Download from_release
    // release_api → Status(404), head_api → {"sha":"abc"} ⇒ from_head
    // both 404 ⇒ Err("There's no public repo at github.com/o/r.")
}
#[test] fn friendly_errors() {
    assert_eq!(error_text(&FetchError::Offline), "fuxx couldn't reach GitHub. Check your connection and try again.");
    assert_eq!(error_text(&FetchError::Status(403)), "GitHub is limiting requests from this Mac for now. Try again in an hour.");
    assert_eq!(error_text(&FetchError::Status(429)), error_text(&FetchError::Status(403)));
    assert_eq!(error_text(&FetchError::Status(500)), "GitHub answered 500. Try again later.");
    assert_eq!(error_text(&FetchError::TooBig), "This plugin is over 100 MB, so fuxx won't download it.");
}
#[test] fn stages_a_real_tarball() {
    // build <tmp>/src/owner-repo-abc/{fuxx-plugin.conf, themes/Sunset} and `tar -czf` it (real /usr/bin/tar);
    // FakeNet serves it; stage() → plugin_dir ends in owner-repo-abc, manifest.id ok, installed passed through
}
#[test] fn stages_a_real_zip_with_files_at_its_root() { /* `ditto -c -k` a folder's contents; find_plugin_root = the unpack dir */ }
#[test] fn a_repo_without_a_manifest_is_not_a_plugin() {
    // tarball with README only ⇒ Err("This repo isn't a fuxx plugin (it has no fuxx-plugin.conf).") and `work` deleted
}
#[test] fn a_checksum_mismatch_is_refused() { /* Download.sha256 = Some("00…") ⇒ Err("The download didn't match its checksum, so fuxx didn't install it.") */ }
#[test] fn too_big_is_refused() { /* a file over MAX_BYTES (sparse via set_len) ⇒ Err(error_text(&TooBig)) */ }
#[test] fn manifest_errors_come_through() { /* tarball with format = 2 ⇒ Err(NewerFormat(2).message()) */ }
```

- [ ] **Step 2: Run** `cargo test --bin fuxx plugin_fetch` — Expected: FAIL.
- [ ] **Step 3: Implement.** `Curl::get`: `/usr/bin/curl -fsSL --max-time 60 -H "Accept: application/vnd.github+json" -w "\n%{http_code}" --url <url>`, status split as `updater::curl_answer` does (copy the logic; `updater.rs` is not changed). `Curl::download`: `-fL --max-time 60 --max-filesize 104857600 -o <to> --url <url>`; curl exit 63 → `TooBig`. `stage`: make `staging/<pid>-<nanos>/`, download to `archive`, refuse over `MAX_BYTES` by `metadata().len()` (chunked downloads have no size header), check `shasum -a 256` when `sha256` is `Some`, unpack into `unpacked/` with `/usr/bin/tar -xzf <archive> -C <dir>` (never `-P`) or `/usr/bin/ditto -x -k <archive> <dir>`, `find_plugin_root`, `manifest::parse` + `check_files`. On any error remove `work` before returning. `find_plugin_root`: the dir if it has `MANIFEST`, else its only non-hidden subdirectory if that has it.
- [ ] **Step 4: Run** `cargo test --bin fuxx plugin_fetch` — Expected: PASS.
- [ ] **Step 5: Checkpoint** as Task 1.

---

### Task 5: Plugin themes in Appearance and the theme list (`plugin_themes.rs`)

**Files:**
- Create: `src/plugin_themes.rs`; Modify: `src/main.rs` (`mod plugin_themes;`)
- Modify: `src/appearance.rs:378-398` (theme reading), `Appearance` struct (two fields), `src/prefs.rs:27` (`theme_choices`), `src/preferences.rs:585-597` (themes and swatches) and `theme_tiles` (`:966`)

**Interfaces:**
- Consumes: `Folders`, `Record` (Task 3); `theme::BUILTIN_THEMES`.
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq)] pub struct PluginTheme { pub name: String, pub plugin: String, pub plugin_id: String, pub path: PathBuf }
  pub fn list(f: &Folders, records: &[Record], taken: &[String]) -> Vec<PluginTheme>;
  pub fn config_without_theme(config: &str, removed: &[String]) -> Option<String>;
  // appearance.rs
  pub plugin_folders: Option<Folders>,            // in Appearance
  pub plugin_themes: Vec<PluginTheme>,             // in Appearance
  pub fn read_theme_text(name: &str, user_dir: Option<&Path>, plugin_themes: &[PluginTheme]) -> Option<String>;
  // prefs.rs
  pub fn theme_choices(user_themes: &[String], plugin_themes: &[String]) -> Vec<String>;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
// plugin_themes.rs
#[test] fn a_taken_name_gets_its_plugin_name() {
    // records: "Sunset Themes" with themes/Sunset, themes/Dracula; taken = ["Dracula", "Mine"]
    // ⇒ names ["Sunset", "Dracula (Sunset Themes)"], plugin "Sunset Themes", path = plugins/<id>/themes/Sunset
}
#[test] fn the_theme_line_goes_when_it_names_a_removed_theme() {
    let removed = ["Sunset".to_string()];
    assert_eq!(config_without_theme("font-size = 13\ntheme = sunset\n", &removed).as_deref(), Some("font-size = 13\n"));
    assert_eq!(config_without_theme("theme = light:Nord,dark:Sunset\n", &removed).as_deref(), Some(""));
    assert_eq!(config_without_theme("theme = Nord\n", &removed), None);
    assert_eq!(config_without_theme("config-file = more\n", &removed), None); // includes untouched
}
// appearance.rs
#[test] fn theme_lookup_order() { /* user dir file "Sunset" wins over a plugin "Sunset"; a plugin theme is read before a built-in of another name; a built-in still reads */ }
// prefs.rs
#[test] fn plugin_themes_join_the_sorted_list() {
    assert_eq!(theme_choices(&["mine".into()], &["Sunset".into()])[..3], ["Default", "Catppuccin Latte", "Catppuccin Mocha"]);
    assert!(theme_choices(&[], &["Sunset".into()]).contains(&"Sunset".to_string()));
}
```

- [ ] **Step 2: Run** `cargo test --bin fuxx theme` — Expected: FAIL (new tests; existing theme tests still pass).
- [ ] **Step 3: Implement.** `config_without_theme`: take the last `theme` entry of `settings::parse(config)`; if its plain value or either `light:`/`dark:` half equals a removed name (ignoring case), return `settings::set_key(config, "theme", &[])`. `appearance::load` builds `Folders::under(home)`, `plugin_store::load`, then `plugin_themes::list(…, taken = user theme names + BUILTIN_THEMES)` before `theme::resolve`, and uses `read_theme_text` (absolute path → user dir → plugin theme by exact listed name → built-in). Preferences builds `themes` with `theme_choices(user, plugin names)`, reads swatches with `read_theme_text`, and keeps `theme_plugins: Vec<Option<String>>` beside `themes`; `theme_tiles` shows that plugin name under the theme name in `ui.text_faint`, `EXPLANATION_SIZE`.
- [ ] **Step 4: Run** `cargo test --bin fuxx` — Expected: PASS (update `pages_follow_note_75` only if a tile assertion depends on the label count).
- [ ] **Step 5: Checkpoint** as Task 1.

---

### Task 6: The `Plugins` global and the three actions (`plugins.rs`)

**Files:**
- Create: `src/plugins.rs`; Modify: `src/main.rs` (`mod plugins;`, `plugins::init(cx)` beside `updater::start_schedule(cx)` at `:1006`)

**Interfaces:**
- Consumes: Tasks 1–5; `appearance::reload(cx)`; `settings::write_atomic` on `Appearance::settings_path`.
- Produces:
  ```rust
  #[derive(Default)] pub struct Plugins { pub records: Vec<Record>, pub updates: HashMap<String, Installed>, pub busy: Option<Busy>, pub message: Option<Message> }
  impl Global for Plugins {}
  #[derive(Clone, Debug, PartialEq)] pub enum Busy { Downloading, Checking, Removing(String), Updating(String) }
  #[derive(Clone, Debug, PartialEq)] pub struct Message { pub text: String, pub error: bool }
  pub const FIRST_CHECK: Duration = Duration::from_secs(15);
  pub fn init(cx: &mut App);                                   // launch_cleanup, load, check after FIRST_CHECK
  pub fn check_updates(cx: &mut App);                          // background; fills `updates` for records whose latest differs
  pub fn can_start(p: &Plugins) -> bool;                       // busy.is_none()
  pub fn install(link: &str, window: &mut Window, cx: &mut App);
  pub fn update(id: &str, window: &mut Window, cx: &mut App);
  pub fn uninstall(id: &str, window: &mut Window, cx: &mut App);
  pub fn install_prompt(m: &Manifest, repo: &Repo) -> (String, String);   // pure: title, detail
  pub fn adds_text(m: &Manifest) -> String;                                 // pure: "Adds 2 themes: Sunset, Sunset Light" / "Adds 1 theme: Sunset"
  ```

- [ ] **Step 1: Write the failing tests** (pure parts; the flows are driven in Task 7's ui-shot and your check)

```rust
#[test] fn the_install_prompt() {
    let (title, detail) = install_prompt(&sunset_manifest(), &Repo { owner: "o".into(), name: "sunset".into() });
    assert_eq!(title, "Install Sunset Themes 1.0.0?");
    assert_eq!(detail, "From github.com/o/sunset\nWarm themes.\nAdds 2 themes: Sunset, Sunset Light");
}
#[test] fn one_operation_at_a_time() {
    let mut p = Plugins::default();
    assert!(can_start(&p));
    p.busy = Some(Busy::Downloading);
    assert!(!can_start(&p));
}
```

- [ ] **Step 2: Run** `cargo test --bin fuxx plugins::` — Expected: FAIL.
- [ ] **Step 3: Implement.** Every action returns at once when `!can_start`. `install`: `parse_link` (None → `message = LINK_HINT` as error) → `busy = Downloading` → background `latest(&Curl, …)` + `stage(…, &folders.staging())` → on main thread `busy = Checking`, `accept(&m, CODE_ALLOWED)` (Err → message, remove `work`), then `window.prompt(PromptLevel::Info, &title, Some(&detail), &["Install", "Cancel"])`; Install → `commit(…, now_unix())`, remove `work`, `appearance::reload(cx)`, `records = load`, `message = "Installed {name}. Its themes are in Appearance."`; Cancel → remove `work`, no message. Always `busy = None` at the end. `update`: as install from the record's repo with `busy = Updating(id)`, no prompt (clicking is the consent; M11z adds the prompt when code appears), message "Updated {name} to {label}.". `uninstall`: `window.prompt(PromptLevel::Warning, "Uninstall {name}?", Some("Its files and its data are deleted."), &["Uninstall", "Cancel"])` → `busy = Removing(id)` → `plugin_store::uninstall`; then if `config_without_theme(main config, the record's listed theme names)` is `Some(text)`, write it atomically; `appearance::reload(cx)`; `message = "Uninstalled {name}."`. Errors become `Message { error: true }`.
- [ ] **Step 4: Run** `cargo test --bin fuxx` — Expected: PASS.
- [ ] **Step 5: Checkpoint** as Task 1.

---

### Task 7: Settings → Plugins page

**Files:**
- Modify: `src/preferences.rs` (`Section::Plugins` after `Sound` in `ALL`, `title` "Plugins", `lede`, `groups`, `Control::PluginInstall`, `Control::PluginList`, labels/explanations/palette entries; a `plugin_link: Entity<NameInput>` field; observe the `Plugins` global and recompute `themes`/`swatches`/`theme_plugins` when it changes; `plugins::check_updates` when the page is selected)
- Create: `src/preferences_plugins.rs` (rendering of the two controls, kept out of the 2,500-line file)
- Modify: `src/ui_shots.rs` (the per-section loop at `:14910` picks the new section up; add a scene with one installed record)

**Interfaces:**
- Consumes: Task 6 (`Plugins`, `install`, `update`, `uninstall`, `can_start`), `Repo::page`.
- Produces: `Section::Plugins`, `Control::PluginInstall`, `Control::PluginList`;
  `pub(crate) fn install_card(view: &PreferencesView, ui: UiColors, cx: &Context<PreferencesView>) -> Div` and
  `pub(crate) fn plugin_cards(ui: UiColors, cx: &Context<PreferencesView>) -> Div` in `preferences_plugins.rs`.

Copy:
- lede: "Add themes from a GitHub link. Uninstalling removes everything a plugin added."
- groups: "Install" → `[PluginInstall]`; "Installed" → `[PluginList]`.
- field hint: "github.com/owner/repo"; button: "Install"; busy lines: "Downloading…", "Checking…", "Removing…", "Updating…".
- a plugin card: name + version (`Installed::label` when it differs from `version`), `github.com/<repo>` (click → `cx.open_url(repo.page())`), description, "N theme(s)"; buttons "Update to <label>" (when `updates` has it), "Uninstall".
- empty: "No plugins yet. Paste a GitHub link above." and a link "How to make a plugin" → `https://github.com/fitzfitz/fuxx-cli#making-a-plugin`.

- [ ] **Step 1: Write the failing tests** — extend `sections_list_every_control_once` and `palette_settings_cover_every_control` for the two controls; a new `plugins_page_follows_note_76` asserting `Section::Plugins.groups()` titles `["Install", "Installed"]` and `Section::ALL.last() == Some(&Section::Plugins)`.
- [ ] **Step 2: Run** `cargo test --bin fuxx preferences` — Expected: FAIL.
- [ ] **Step 3: Implement** in M11x's card style (`card`, `row`, `button`, `toggle` helpers already in `preferences.rs`); Enter in the link field = Install; buttons render disabled (faint, no click) while `!can_start`; the message line under the install card in `ui.text_dim`, errors in the colour the Sound page uses for its problem line.
- [ ] **Step 4: Run** `cargo test --bin fuxx && cargo run --features ui-shots -- __ui-shots preferences` — Expected: tests PASS; every `preferences-*` scene passes, including the Plugins ones; open the PNGs in `target/ui-shots/` and check the page in a dark and a light theme.
- [ ] **Step 5: Checkpoint** as Task 1, plus `cargo fmt --check`.

---

### Task 8: An end-to-end test, "Making a plugin", the sample plugin, docs

**Files:**
- Create: `examples/plugins/sunset-themes/fuxx-plugin.conf`, `examples/plugins/sunset-themes/themes/Sunset`, `examples/plugins/sunset-themes/themes/Sunset Light` (warm palettes in fuxx's theme format)
- Modify: `README.md` (new section `## Making a plugin`), `src/plugin_fetch.rs` (test only)
- Modify (docs): `docs/04-Development/76 - Milestones 11y-11z - Plugins.md` (status + on-screen checklist), `docs/04-Development/40 - Roadmap.md`, `docs/04-Development/Plans/00 - Plan Tracker (Start Here).md`, `docs/06-Progress-Log/60 - Progress Log.md`

- [ ] **Step 1: Write the end-to-end test** `example_plugin_installs_and_uninstalls_cleanly`: tar `examples/plugins/sunset-themes` the way GitHub does (`owner-repo-sha/` top folder), `FakeNet` serves a head commit and that tarball, run `latest` → `stage` → `accept` → `commit` → `plugin_themes::list` (two themes, readable through `read_theme_text`) → `uninstall` → both folders hold nothing of it.
- [ ] **Step 2: Run** `cargo test --bin fuxx example_plugin` — Expected: PASS (it exercises Tasks 1–5 together; a failure here is a real integration bug: fix it in the owning task's module).
- [ ] **Step 3: Write README's "Making a plugin"**: what a plugin is (a public GitHub repo with `fuxx-plugin.conf`), the theme example from note 76 §2 verbatim, "push it and share the link", the key table, the rule "keep any data in `$FUXX_PLUGIN_DATA`; fuxx can only remove what's in its folders", and that plugins that run programs come in a later version.
- [ ] **Step 4: Ask the user** before creating anything on GitHub: "To check install from a real link, may I create the public repo `fitzfitz/fuxx-sunset-themes` from `examples/plugins/sunset-themes`?" Without a yes, the on-screen check uses any theme repo the user picks.
- [ ] **Step 5: Docs.** Note 76: status → built (M11y), with the on-screen checklist: (1) Settings → Plugins shows the empty state and the "How to make a plugin" link; (2) a bad link shows the hint; (3) pasting the sample repo shows the confirmation with "Adds 2 themes: Sunset, Sunset Light"; (4) Install → both themes appear in Appearance with "Sunset Themes" under them; (5) choose Sunset, it applies live; (6) Uninstall → confirmation → fuxx returns to the default theme live, the `theme` line is gone from `~/.config/fuxx/config`, and `ls ~/Library/Application\ Support/fuxx/plugins` shows only `plugins.json`; (7) after a push to the sample repo, reopening Settings → Plugins shows "Update to …". Roadmap, Plan Tracker and a Progress Log entry: M11y built, waiting for your check.
- [ ] **Step 6: Verify** `cargo test --bin fuxx && cargo clippy --all-targets && cargo fmt --check && cargo run --features ui-shots -- __ui-shots` — Expected: all tests pass, clippy shows nothing new (14 bin / 27 test warnings before), fmt clean, every scene passes. Then hand over for the on-screen check; commit only on the user's word.
