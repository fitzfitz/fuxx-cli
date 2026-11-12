# M11z — Helper-Program Plugins Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A plugin with `bin` and/or `zsh` installs from a GitHub link (with an honest "runs a program" warning), loads into fuxx's zsh terminals after your `.zshrc`, has an On/Off switch, and uninstalls completely.

**Architecture:** M11y's plugin core stays; `CODE_ALLOWED` flips on. A new pure module `plugin_shell` turns the enabled records into `plugins.zsh`; fuxx writes it beside its zsh integration after every plugin change. The zsh integration gains a *plugin mode*: when a helper is on, fuxx's `.zshenv` keeps `ZDOTDIR` on fuxx's folder, and two pass-through files (`.zprofile`, `.zshrc`) run yours and then source `plugins.zsh` at top level. The command marks become their own switch (`FUXX_ZSH_MARKS`), so turning command blocks off no longer turns plugins off.

**Tech Stack:** Rust, GPUI (pinned rev), zsh 5.1+ scripts compiled into the binary (`include_str!`), `/usr/bin/lipo` for the Mac-chip check. **No new crate.**

**Spec:** [[76 - Milestones 11y-11z - Plugins]] §2, §3 (helper rules), §4 (warning), §5 (re-prompt), §6 (switch), §8 ("terminals already open"), §9 (how it runs), §12, and the M11z row. M11y's plan: [[M11y Plan - Plugins and Theme Plugins]].

**Branch:** `m11z-helper-plugins` from `master` (`b2c9492`). Implementers commit per task on the branch; nothing is merged or pushed until you say so.

## Global Constraints

- **zsh only.** bash and fish terminals in fuxx get no helper plugins.
- `plugins.zsh` lives at `~/Library/Application Support/fuxx/shell-integration/zsh/plugins.zsh`, regenerated (atomically) on install, update, uninstall, On/Off and at launch. For each **enabled** helper, in record order: `path=(<plugin>/<bin> $path)` when it has `bin`; `FUXX_PLUGIN_DIR` and `FUXX_PLUGIN_DATA` exported only while its `zsh` scripts are sourced, then unset. All paths single-quoted (`Application Support` has a space).
- `FUXX_PLUGIN_DATA` = `~/Library/Application Support/fuxx/plugin-data/<id>/`, created by fuxx at install for a plugin that runs code.
- Plugins load **once, after your `.zshrc`, at top level** (a plain `typeset` in a plugin stays global). Interactive shells only. With no helper on, the handoff is exactly M11l's.
- Integration is injected when `command-blocks` is on **or** any enabled helper exists; the marks run only when `command-blocks` is on.
- Install warning (spec §4, verbatim): "Runs a program on your Mac in every new terminal. fuxx can't limit what it does, so only install plugins from people you trust." The prompt is `PromptLevel::Warning` for a plugin that runs code. No permission list.
- A `bin` with no file executable for this Mac → "This plugin has no program for this Mac." (an executable file: a script with `#!`, or a Mach-O whose `lipo -archs` includes this chip).
- An update that newly runs code shows the full warning prompt again (spec §5).
- After install, update, uninstall or On/Off of a plugin that runs code, the page says: "New terminals get the change; terminals already open keep it until you close them."
- `Record` gains fields with `#[serde(default)]` so an M11y `plugins.json` still reads (never "damaged").
- No new crate. Copy fixed by this plan (quoted strings); keep it exactly.

## Review Focus

- **Your own startup files still run, in order, exactly once** in plugin mode: `.zshenv`, `.zprofile` (login), `.zshrc`, `.zlogin` (login), with and without your own `ZDOTDIR`, and when your `.zshenv` sets `ZDOTDIR` itself. A broken handoff would skip your dotfiles. Test in Task 3.
- **`$ZDOTDIR` inside your `.zprofile` and `.zshrc`** is your folder, not fuxx's, so dotfiles that `source $ZDOTDIR/…` keep working. Test in Task 3.
- **A plugin script that errors** (syntax error, `false` at top level, a missing command) prints its error and the prompt still comes; the next plugin still loads; command marks still work. Test in Task 3.
- **Turning command blocks off** with a helper on: plugins still load, no OSC 133 marks are printed. Test in Task 3.
- **An M11y `plugins.json`** (no `enabled`/`bin`/`zsh` fields) loads with every theme plugin intact. Test in Task 1.

---

### Task 1: Code plugins in the manifest and the record

**Files:** Modify `src/plugin_manifest.rs`, `src/plugin_store.rs`, `src/plugins.rs` (prompt only).

**Interfaces:**
- Produces: `plugin_manifest::CODE_ALLOWED = true`; `ManifestError::NoProgramForThisMac` with message "This plugin has no program for this Mac."; `Record { …, #[serde(default = "on")] pub enabled: bool, #[serde(default)] pub bin: Option<String>, #[serde(default)] pub zsh: Vec<String> }` and `impl Record { pub fn runs_code(&self) -> bool }`; `plugin_store::commit` fills them from the manifest, keeps `enabled` on update, creates `data_of(id)` when the manifest runs code; `plugin_store::set_enabled(f: &Folders, id: &str, on: bool) -> Result<Record, String>`; `plugins::install_prompt` returns a third value, `PromptLevel`, and appends the warning line for code.

- [ ] **Step 1: Write the failing tests**
  - `an_m11y_record_file_still_reads`: a literal `plugins.json` from M11y (no new fields) → `load` gives the record with `enabled == true`, `bin == None`, `zsh == []`.
  - `a_bin_needs_a_program_for_this_mac`: a `bin/` with only a non-executable file → `NoProgramForThisMac`; with an executable `#!/bin/sh` script → Ok; with a Mach-O built for the other chip only (copy `/usr/bin/true` and thin it with `lipo -thin <other> -output`) → `NoProgramForThisMac`; `/usr/bin/true` copied as is (universal) → Ok.
  - `code_plugins_are_accepted_now`: `accept(&m_with_zsh, CODE_ALLOWED)` is Ok.
  - `install_creates_the_data_folder_for_code_only`: commit a helper → `data_of(id)` exists; commit a theme plugin → it doesn't.
  - `set_enabled_flips_and_survives_update`: set off, then commit an update of the same id → still off.
  - `the_prompt_warns_about_code`: for a zsh manifest the detail's last line is the warning (verbatim) and the level is `Warning`; for a theme manifest unchanged and `Info`.
- [ ] **Step 2: Run** `cargo test --bin fuxx plugin_` — FAIL.
- [ ] **Step 3: Implement.** The chip check runs in `check_files` when `bin` is set: at least one regular file under `bin/` (not recursive) with an execute bit that either starts with `#!` or is a Mach-O (`0xfeedfacf`/`0xcafebabe` magic) for which `/usr/bin/lipo -archs <file>` lists `plugin_source::chip()`.
- [ ] **Step 4: Run** `cargo test --bin fuxx` — PASS.
- [ ] **Step 5: Commit.**

### Task 2: `plugins.zsh` (`plugin_shell.rs`)

**Files:** Create `src/plugin_shell.rs`; modify `src/main.rs` (`mod plugin_shell;`).

**Interfaces:**
- Produces: `pub fn zsh_quote(s: &str) -> String` (single quotes, `'` → `'\''`); `pub fn script(f: &Folders, records: &[Record]) -> String`; `pub fn any_enabled(records: &[Record]) -> bool` (enabled and runs code); `pub fn write(dir: &Path, f: &Folders, records: &[Record]) -> io::Result<()>` (writes `<dir>/zsh/plugins.zsh` beside the integration, temp file + rename, `0o644`).

- [ ] **Step 1: Write the failing tests**
  - `script_is_empty_with_no_helpers` → exactly the header comment `# fuxx plugins (generated by fuxx; edits are replaced)\n`.
  - `script_loads_enabled_helpers_in_order`: two helpers (one off) and a theme plugin → only the on helper appears; its lines in order: `path=('<plugins>/<id>/bin' $path)`, `export FUXX_PLUGIN_DIR='<plugins>/<id>' FUXX_PLUGIN_DATA='<data>/<id>'`, `builtin source -- '<plugins>/<id>/shell/a.zsh'`, `unset FUXX_PLUGIN_DIR FUXX_PLUGIN_DATA`.
  - `paths_with_spaces_and_quotes_are_quoted`: `zsh_quote("/a b/it's")` == `'/a b/it'\''s'`.
  - `write_is_atomic_and_0644`: writes, mode `0o644`, no `.tmp` left.
- [ ] **Steps 2–5:** run (FAIL), implement, run (PASS), commit.

### Task 3: Plugin mode in the zsh integration

**Files:** Modify `assets/shell-integration/zsh/.zshenv`; create `assets/shell-integration/zsh/.zprofile` and `assets/shell-integration/zsh/.zshrc`; modify `src/shell_integration.rs` (`FILES` gains the two, `env_for`), `src/workspace.rs` (`session_env` / `zsh_integration` callers).

**Interfaces:**
- Produces: `pub fn env_for(shell: &Path, user_zdotdir: Option<&OsStr>, marks: bool, plugins: bool, dir: Option<&Path>) -> Vec<(String, String)>`: nothing unless the shell is `zsh`, `dir` is Some and `marks || plugins`; then `ZDOTDIR=<dir>/zsh`, `FUXX_ZSH_ZDOTDIR` as today, `FUXX_ZSH_MARKS=1` when `marks`, `FUXX_ZSH_PLUGINS=1` when `plugins`. `add_login_shell_env` takes the same two flags.
- Script contract:
  - `.zshenv`: as today, then if interactive: source `fuxx-integration.zsh` only when `FUXX_ZSH_MARKS` is set; when `FUXX_ZSH_PLUGINS` is set, record your final `ZDOTDIR` (set or unset, after your `.zshenv` ran) in `_fuxx_zdotdir` / `_fuxx_zdotdir_set`, and set `ZDOTDIR` back to fuxx's folder. Unset both `FUXX_ZSH_*` so nested shells start clean.
  - `.zprofile`: `ZDOTDIR` = yours while your `.zprofile` (from `${ZDOTDIR-$HOME}`) is sourced, then fuxx's again.
  - `.zshrc`: put your `ZDOTDIR` back exactly (set or unset), source your `.zshrc`, then `builtin source -- <dir>/plugins.zsh` at top level if readable; unset `_fuxx_zdotdir*`. Your `.zlogin` is then read by zsh from your folder.
  - Everything quoted as in today's `.zshenv`; errors in your files or a plugin never stop the next step (`{ … } always { … }`).

- [ ] **Step 1: Write the failing real-zsh tests** (extend the existing `spawn_zsh` helper with the two flags; `startup_files` already logs env/profile/rc/login):
  - `plugin_mode_runs_your_files_once_in_order`: login interactive, with no `ZDOTDIR`, with a `ZDOTDIR`, and with a `.zshenv` that sets `ZDOTDIR` → the log reads `env:profile:rc:login` exactly once each; `$ZDOTDIR` printed at the prompt equals yours (or is unset).
  - `your_files_see_your_zdotdir`: a `.zprofile` and `.zshrc` that print `$ZDOTDIR` → yours both times.
  - `plugins_load_after_your_zshrc_at_top_level`: a plugin script doing `typeset X=1; print -r -- "plugin:$T_RC"` → prints `plugin:1` (after `.zshrc` set `T_RC`) and `$X` is `1` at the prompt.
  - `a_broken_plugin_doesnt_stop_the_prompt`: first script has a syntax error, second sets `Y=2` → the prompt comes, `$Y` is `2`, marks still arrive.
  - `plugins_without_command_blocks_send_no_marks`: `marks=false, plugins=true` → plugin loads, `marks_in(...)` is empty.
  - `no_plugins_is_m11l_exactly`: `plugins=false` → the existing `real_zsh_keeps_your_zdotdir_and_files` assertions hold and fuxx's `.zprofile`/`.zshrc` are never read (a marker variable they'd set is absent).
  - `env_for_flags`: the four combinations of `marks`/`plugins` → the expected keys.
- [ ] **Step 2: Run** `cargo test --bin fuxx shell_integration` — FAIL.
- [ ] **Step 3: Implement** the scripts and `env_for`; `workspace::zsh_integration` returns the folder when `command_blocks || plugin_shell::any_enabled(&Plugins.records)` (use `try_global`), and `session_env` passes `marks = command_blocks`, `plugins = any_enabled`.
- [ ] **Step 4: Run** `cargo test --bin fuxx` — PASS (all existing M11l real-zsh tests unchanged).
- [ ] **Step 5: Commit.**

### Task 4: Wiring the actions

**Files:** Modify `src/plugins.rs`, `src/main.rs`.

**Interfaces:**
- Produces: `pub fn set_enabled(id: &str, on: bool, cx: &mut App)` (background: `plugin_store::set_enabled`, then the same refresh as other actions); every refresh and `init` (after launch cleanup) call `plugin_shell::write(integration_dir, folders, records)` off the UI thread; `install`/`update` use the prompt level from Task 1; `update` re-prompts with the full warning when the new manifest runs code and the record didn't; the success message for a code plugin gets the line "New terminals get the change; terminals already open keep it until you close them." appended.
- [ ] **Step 1: Write the failing tests** (pure parts): `update_needs_a_prompt(record: &Record, m: &Manifest) -> bool` (true only when `m.runs_code() && !record.runs_code()`); `success_text(name, verb, runs_code)` returns the exact strings.
- [ ] **Steps 2–5:** run (FAIL), implement, run (PASS; full suite), commit.

### Task 5: The page

**Files:** Modify `src/preferences_plugins.rs`, `src/ui_shots.rs`.

- Card meta for a code plugin ends with "Runs a program" (warning colour, as the Sound page's problem line); an On/Off `toggle` (the existing helper) left of Update/Uninstall, only for code plugins, calling `plugins::set_enabled`; disabled while `!can_start`.
- ui-shot: extend the installed scene with a second record that runs code (one on, one off) and assert the two shots differ where the switch is.
- [ ] **Steps:** extend `plugins_page_follows_note_76` for the label; run (FAIL); implement; `cargo test --bin fuxx` and `cargo run --features ui-shots -- __ui-shots preferences` (PASS); commit.

### Task 6: A test helper, end to end, docs

**Files:** Create `examples/plugins/hello-helper/{fuxx-plugin.conf, bin/fuxx-hello, shell/hello.zsh}`; modify `src/plugin_fetch.rs` (test), `README.md`, docs.

- Sample: id `hello-helper`, name `Hello Helper`, version `1.0.0`, description `A tiny test plugin: a fuxx-hello command and a FUXX_HELLO variable.`, `bin = bin`, `zsh = shell/hello.zsh`. `bin/fuxx-hello` (`#!/bin/sh`, mode 0755) prints `hello from a fuxx plugin`; `shell/hello.zsh` sets `typeset -g FUXX_HELLO=1` and keeps `$FUXX_PLUGIN_DATA` in `typeset -g FUXX_HELLO_DATA`.
- [ ] **Step 1:** e2e test `helper_plugin_loads_and_uninstalls_cleanly`: tar the sample GitHub-style, install through `latest`/`stage`/`accept`/`commit`, `plugin_shell::write`, spawn a real zsh in plugin mode (the Task 3 helper) → `fuxx-hello` prints its line, `$FUXX_HELLO` is `1`, `$FUXX_HELLO_DATA` is `plugin-data/hello-helper`; `set_enabled(false)` + write → a new zsh has neither; uninstall → `plugins.zsh` holds only the header and both folders are gone.
- [ ] **Step 2:** README "Making a plugin": replace "plugins that run programs come in a later version" with a short helper section (the `bin`/`zsh` keys, `$FUXX_PLUGIN_DIR`/`$FUXX_PLUGIN_DATA`, zsh only, the warning users see, prebuilt binaries in a release zip `fuxx-plugin-macos-<arm64|x86_64>.zip` or universal).
- [ ] **Step 3: Ask the user** before creating anything on GitHub: "May I create the public repo `fitzfitz/fuxx-hello-helper` from `examples/plugins/hello-helper` for your on-screen check?"
- [ ] **Step 4: Docs.** Note 76: M11z status → built, with its on-screen checklist: (1) install the helper repo → the warning prompt; (2) a new terminal: `fuxx-hello` works; (3) switch it off → a new terminal: `command not found`, the message about open terminals; (4) turn command blocks off with the helper on → it still loads; (5) uninstall → `plugins.zsh` empty, both folders gone. Roadmap, Plan Tracker, Progress Log.
- [ ] **Step 5: Verify** `cargo test --bin fuxx && cargo clippy --all-targets && cargo fmt --check && cargo run --features ui-shots -- __ui-shots preferences` (all pass; clippy 14/27; the `title-bar-sliding` scene's pre-existing failure is not M11z's). Commit.
