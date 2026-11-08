---
tags: [development, plan, milestone]
milestone: M11e
status: built
updated: 2026-11-08
---

# M11e Plan — Update Available

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[57 - Milestone 11e - Update Available]] (the spec) |
| **Status** | 🟨 Tasks 0–5 built and reviewed 2026-11-08 (834 tests); Tasks 6–8 need your word / your eyes |
| **Tasks done** | 8 / 9 (Tasks 0–7); Task 9 docs partly |
| **▶ Pick up here** | Task 8: release v0.3.1 on your word, then the on-screen checklist |
| **Decisions used** | Yours (2026-11-08): an updater without M12; fuxx's own (not Sparkle); a blue rounded "Update Available" above the bottom bar; releases by GitHub Actions on a tag; skip the questions. The rest: AI's call, in note 57. |
| **Needs your eyes** | 👀 Task 0 (spike), Task 7 (first release, your word), Task 8 (on screen) |

**Goal:** fuxx finds newer GitHub Releases, shows a blue Update Available button above the sidebar footer, and on a
click downloads, checks, swaps and reopens itself; releases are built by GitHub Actions on a `v*` tag.

**Architecture:** `update_release.rs` is pure. It holds versions, the GitHub JSON, every sheet's copy, and the swap
script's text. `updater.rs` holds the effects (curl, shasum, ditto, plutil, codesign, the detached swap script) and a
GPUI Global `Updates` with the state the sidebar draws. The check runs on the background executor. Sheets use
`window.prompt`, as M11d's Notifications Status does. The release side is one workflow plus a tag-check script.

**Tech Stack:** GPUI at the pinned rev, `serde_json` (already a dependency), macOS command-line tools, GitHub Actions
(`actions/checkout@v4`, the runner's `gh`).

**Spec:** [[57 - Milestone 11e - Update Available]]

## Global Constraints

- **No new crates.** External tools, by absolute path only: `/usr/bin/curl`, `/usr/bin/ditto`, `/usr/bin/shasum`,
  `/usr/bin/plutil`, `/usr/bin/codesign`, `/usr/bin/xattr`, `/usr/bin/open`, `/bin/sh`.
- API URL `https://api.github.com/repos/fitzfitz/fuxx-cli/releases/latest`; release page fallback
  `https://github.com/fitzfitz/fuxx-cli/releases/latest`. Asset name `fuxx-{version}.zip`. Tag `v{version}`.
  Bundle id `io.github.fitzfitz.fuxx` (`native_notify::BUNDLE_ID`).
- Timings: first check **10 s** after launch, then every **24 h**; `curl --max-time 20` for the check, `--max-time 600`
  for the download; the swap script waits at most **60 s**.
- Paths: cache `~/Library/Caches/fuxx/update/` (emptied before each download); the old app is kept as
  `fuxx.app.previous` beside the app.
- Settings line `check-updates = off` (default on: no line). Preferences → General switch label `Check for updates`,
  note `Asks GitHub once a day whether a newer fuxx is out.`
- Button: text `Update Available` / `Downloading update…`; fill `#1F6FEB`, hover `#1A5FCC`, white text, height 28 px,
  radius 8 px, `ROW_INSET` sides, 8 px above the footer's top edge.
- Copy: exactly as note 57: `fuxx {v} is available`, `You have {current}.`, notes' first **6** lines,
  `Restarting ends {n} session(s).`, buttons `Update and Restart` · `Later`; `Download from GitHub…`;
  `fuxx {v} is the newest version.`; `Couldn't check for updates`; `The update couldn't be installed` with
  `Open Release Page` · `OK`. Menu item `Check for Updates…` in the fuxx menu above `Hide fuxx`.
- Only a bundled fuxx (`native_notify::is_bundled()`) checks automatically; the menu item works everywhere, but an
  unbundled fuxx only offers "Download from GitHub…".
- Nothing blocks the UI thread. Claude never pushes a tag or a branch without your word.
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo clippy --all-targets` (both feature sets),
  `cargo fmt --check`, `cargo run --features ui-shots -- __ui-shots [filter]`, `scripts/bundle-mac.sh --app-only`.

## Review Focus

1. **Offline, rate-limited (HTTP 403) or odd JSON.** Expected: the automatic check is silent, the manual check says
   "Couldn't check for updates" plus the reason, and nothing panics. (Task 2: `parse_latest` error cases; Task 3:
   `check_outcome` over `Err`.)
2. **A corrupt or tampered download** (digest mismatch, wrong bundle id, wrong version, two apps in the zip, a bad
   signature). Expected: nothing is swapped, the installed app is untouched, and the error sheet names the reason.
   (Task 5: `verify` tests.)
3. **fuxx running from somewhere it can't replace itself** (a read-only folder, a disk image under `/Volumes`, an
   App Translocation path, `cargo run`). Expected: "Download from GitHub…" opens the release page. (Task 4:
   `self_update_target` tests.)
4. **The swap failing halfway** (the move of the new app fails). Expected: the previous app is put back and opened.
   (Task 5: `swap_script` run for real in a temp dir with a failing move.)
5. **Version edge cases:** `v0.10.0` vs `0.9.0`, an equal version, an older release, a tag like `v1.2` or `v1.2.3-beta`.
   Expected: only a strictly newer `x.y.z` shows the button; malformed tags are ignored. (Task 2: `parse_version` /
   `is_newer` tests.)

---

### Task 0: Spike on your Mac — no "Open Anyway", permission survives — ✅ (2026-11-08, both pass)

Throwaway, run by Claude with you watching; it also installs the M11d build into `~/Applications`. Claude serves
`target/dist/fuxx.app` zipped (with `ditto -c -k --keepParent`) from `python3 -m http.server` on localhost. It
downloads that with `/usr/bin/curl` into a scratch folder, unzips it with `ditto -x -k`, checks
`xattr -l` shows no `com.apple.quarantine`, renames `~/Applications/fuxx.app` to `fuxx.app.previous`, moves the new
one in, and opens it.
- [x] You quit fuxx first; Claude runs the spike.
- [x] Check 1: it opens with no "Open Anyway" (note 57 item 1).
- [x] Check 2: Help → Notifications Status… says fuxx sends its own notifications, and macOS didn't ask again
  (item 2).
- [x] Record the result in note 57. If a check fails, stop and redesign before Task 1. If both pass, delete
  `~/Applications/fuxx.app.previous` on your word.

### Task 1: Version 0.3.0 and the Release workflow — ✅

**Files:**
- Create: `.github/workflows/release.yml`, `scripts/check-release-tag.sh`
- Modify: `Cargo.toml` (`version = "0.3.0"`), `Cargo.lock`

**Interfaces:**
- Produces: `scripts/check-release-tag.sh <tag>`, which exits 0 when `<tag>` is `v` + `cargo pkgid`'s version, and
  otherwise exits 1 with `tag <tag> doesn't match Cargo.toml version <v>`.
- Workflow `release.yml`: `on: push: tags: ['v*']`, `permissions: contents: write`, one job on `macos-15`. Steps:
  1. checkout;
  2. `rustup target add aarch64-apple-darwin x86_64-apple-darwin`;
  3. `scripts/check-release-tag.sh "$GITHUB_REF_NAME"`;
  4. `cargo test`;
  5. `scripts/bundle-mac.sh --app-only`;
  6. `ditto -c -k --keepParent target/dist/fuxx.app "target/dist/fuxx-$V.zip"`;
  7. `gh release create "$GITHUB_REF_NAME" "target/dist/fuxx-$V.zip" --title "fuxx $V" --generate-notes`, with
     `GH_TOKEN: ${{ github.token }}`.

- [x] **Step 1: failing check**: run `scripts/check-release-tag.sh v0.3.0` → fails (the script doesn't exist yet).
- [x] **Step 2: implement** the script, the version bump and the workflow.
- [x] **Step 3: verify**:
  - `scripts/check-release-tag.sh v0.3.0` exits 0;
  - `scripts/check-release-tag.sh v0.3.1` and `scripts/check-release-tag.sh 0.3.0` exit 1 with the message;
  - the workflow parses (`ruby -ryaml -e 'YAML.load_file(".github/workflows/release.yml")'`);
  - `cargo test` passes;
  - `scripts/bundle-mac.sh --app-only` builds an app whose `Info.plist` says `0.3.0`.
- [x] **Step 4: commit** `feat(m11e): version 0.3.0 and the Release workflow (GitHub Actions on a v* tag)`.

### Task 2: Releases and versions (pure) — ✅

**Files:** Create `src/update_release.rs`; Modify `src/main.rs` (`mod`).

**Interfaces:**
- Produces: `#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)] pub struct Version(pub u64, pub u64, pub u64)`
  with `Display` (`0.3.0`); `pub fn parse_version(s: &str) -> Option<Version>` (accepts `0.3.0` and `v0.3.0` only);
  `pub fn current() -> Version` (from `CARGO_PKG_VERSION`); `pub fn is_newer(release: Version, running: Version) -> bool`;
  `#[derive(Clone, Debug, PartialEq)] pub struct Release { pub version: Version, pub page_url: String, pub notes: String, pub zip: Option<Asset> }`;
  `#[derive(Clone, Debug, PartialEq)] pub struct Asset { pub url: String, pub sha256: Option<String> }` (lowercase hex
  from `digest` `"sha256:…"`); `pub fn parse_latest(json: &str) -> Result<Release, String>` (uses `tag_name`,
  `html_url`, `body`, and the asset named exactly `fuxx-{version}.zip`);
  `pub fn notes_excerpt(body: &str, lines: usize) -> String` (first non-empty lines).
- [x] **Step 1: failing tests**:
  - `parse_version`: `"0.3.0"` and `"v0.3.0"` → `Version(0,3,0)`; `"v1.2"`, `"1.2.3-beta"`, `""`, `"x.y.z"` → `None`.
  - `is_newer`: `(0,10,0)` vs `(0,9,0)` is true; equal and older are false.
  - `parse_latest`: on a sample JSON (tag `v0.3.1`, two assets incl. `fuxx-0.3.1.zip` with
    `digest "sha256:ABC123"`) → version, page URL, `sha256 == Some("abc123")`. With no matching asset → `zip: None`.
    With no digest → `sha256: None`.
  - Errors: GitHub's error JSON (`{"message":"API rate limit exceeded"}`) → `Err` containing that message; invalid
    JSON → `Err`; a bad tag → `Err("unrecognised version …")`.
  - `notes_excerpt`: 6 of 10 lines, blank lines skipped.
- [x] **Step 2: run** `cargo test --bin fuxx update_release` → FAIL.  **Step 3: implement.**  **Step 4: run**
  `cargo test`, clippy (both), fmt → ok.
- [x] **Step 5: commit** `feat(m11e): releases and versions — the GitHub JSON, version order (pure)`.

### Task 3: Checking — automatic, the menu item, the setting — ✅

**Files:** Create `src/updater.rs`; Modify `src/update_release.rs` (copy helpers), `src/main.rs` (action
`CheckForUpdates` in `actions!(fuxx, ..)`, its menu item, the app-level handler, starting the schedule),
`src/appearance.rs` + `src/prefs.rs` + `src/preferences.rs` (`check_updates`, `OnOff::CheckUpdates` under General).

**Interfaces:**
- Produces (updater): `#[derive(Clone, Debug, PartialEq)] pub enum UpdateState { Idle, Available(Release), Downloading }`;
  `pub struct Updates { pub state: UpdateState }` (`impl Global`; `cx.set_global(Updates { state: Idle })` at launch);
  `pub fn fetch_latest() -> Result<String, String>` (blocking curl `-fsSL --max-time 20`, with header
  `Accept: application/vnd.github+json`; a non-2xx status → `Err("GitHub answered {code}")`; curl failure →
  `Err("offline or unreachable")`); `pub fn check(manual: bool, cx: &mut App)` (background fetch + parse, then on the
  main thread: newer → `Available`, notify windows; a manual check shows its sheet in `help_target`'s window);
  `pub fn start_schedule(cx: &mut App)` (bundled and `Appearance.check_updates` only: 10 s, then every 24 h,
  re-reading the setting each time).
- Produces (update_release): `pub enum CheckOutcome { Newer(Release), UpToDate(Version), Failed(String) }`;
  `pub fn check_outcome(result: Result<Release, String>, running: Version) -> CheckOutcome`;
  `pub fn up_to_date_title(v: Version) -> String` (`fuxx {v} is the newest version.`);
  `pub const CHECK_FAILED: &str = "Couldn't check for updates"`.
- [x] **Step 1: failing tests**:
  - `check_outcome` over a newer, an equal and an older release, and `Err("offline or unreachable")`.
  - The appearance setting: none → on; `off` → off; junk → on, with a warning, like `browser_keep_data`'s test.
  - `prefs::check_updates_values(true) == []` and `(false) == ["off"]`.
  - The menu test: the fuxx menu reads `Check for Updates…`, then `Hide fuxx` (with a separator after it, as the
    file's style has).
  - A Preferences ui-shot of General showing the switch and its note.
- [x] **Step 2: run** → FAIL.  **Step 3: implement.**  **Step 4: run** `cargo test`, clippy (both), fmt,
  `__ui-shots preferences` → ok.
- [x] **Step 5: commit** `feat(m11e): check for updates — at launch, daily, and from the fuxx menu`.

### Task 4: The Update Available button and its sheet — ✅

**Files:** Modify `src/sidebar.rs` (the button above `render_footer`), `src/update_release.rs` (offer copy),
`src/updater.rs` (`show_offer`), `src/ui_shots.rs` (scene `update-button`).

**Interfaces:**
- Consumes: `Updates`, `UpdateState`, `Release` (Tasks 2–3).
- Produces: `pub struct Offer { pub title: String, pub detail: String, pub buttons: Vec<&'static str>, pub self_update: bool }`;
  `pub fn offer(release: &Release, running: Version, ending_sessions: usize, can_self_update: bool) -> Offer` (pure;
  the detail is `You have {running}.`, a blank line, the notes excerpt, then `Restarting ends {n} session(s).` only
  when `n > 0`; buttons `["Update and Restart", "Later"]`, or `["Download from GitHub…", "Later"]` when it can't
  self-update); `updater::show_offer(window, cx)`;
  `pub fn self_update_target(exe: &Path, writable: impl Fn(&Path) -> bool) -> Option<PathBuf>` (pure). It returns
  the `….app` folder three levels above `exe`, but only if it ends in `fuxx.app`, isn't under `/Volumes/` or a path
  containing `/AppTranslocation/`, and its parent is writable. `can_self_update` = bundled and this is `Some`.
  `ending_sessions` is the number of open terminal sessions when persistence is off, and 0 when it is on.
  Update and Restart calls `updater::install(release, cx)` (Task 5; in this task it is a stub that opens the
  release page). Download from GitHub… opens `release.page_url`.
- [x] **Step 1: failing tests**:
  - `offer` with 0 and with 4 ending sessions (`Restarting ends 4 sessions.`), and 1 (`… 1 session.`).
  - `can_self_update` false gives `Download from GitHub…`.
  - `self_update_target`: `~/Applications/fuxx.app/Contents/MacOS/fuxx` with a writable parent → that app;
    a non-writable parent → None; `/Volumes/fuxx 0.3.0/fuxx.app/…` → None; an AppTranslocation path → None;
    `target/debug/fuxx` → None.
  - The notes are cut to 6 lines.
  - ui-shot `update-button`, dark and light:
    - `Idle` has no button;
    - `Available` shows the button, 28 px tall, 8 px above the footer, with fill `#1F6FEB`;
    - `Downloading` reads `Downloading update…` and ignores a click (the state is unchanged).
- [x] **Step 2: run** → FAIL.  **Step 3: implement.**  **Step 4: run** `cargo test`, clippy (both), fmt, full
  `__ui-shots`. Look at the PNGs.
- [x] **Step 5: commit** `feat(m11e): the blue Update Available button and the update sheet`.

### Task 5: Download, check, swap, reopen — ✅

**Files:** Modify `src/updater.rs`, `src/update_release.rs` (script text, pure checks), `src/main.rs` (launch cleanup
of `fuxx.app.previous`).

**Interfaces:**
- Consumes: `self_update_target` (Task 4).
- Produces (update_release, pure): `pub struct Unpacked { pub bundle_id: String, pub version: String, pub signature_ok: bool, pub apps: usize }`;
  `pub fn verify(expected: Version, sha256_expected: &str, sha256_actual: &str, u: &Unpacked) -> Result<(), String>`
  (reasons: `the download is damaged (checksum)`, `the zip holds {n} apps`, `not fuxx ({id})`,
  `version {v} instead of {expected}`, `the app's signature is broken`);
  `pub fn swap_script(pid: u32, app: &Path, new_app: &Path, opener: &str) -> String` (POSIX sh; `opener` is
  `/usr/bin/open`, or `true` in tests).
- Produces (updater): `pub fn install(release: Release, cx: &mut App)`. It sets `Downloading`, then on the background
  executor: empty the cache dir, curl `-fL --max-time 600`, `shasum -a 256`, `ditto -x -k`, count the `*.app`, read
  `CFBundleIdentifier` and `CFBundleShortVersionString` with `plutil -extract … raw`, and run `codesign --verify --deep --strict`.
  Then `verify`. On success: write the script, spawn `/bin/sh <script>` with `process_group(0)` and null stdio,
  then `cx.quit()`. On failure: state back to `Available`, plus the error sheet (`Open Release Page` · `OK`).
  `pub fn clean_previous(app: &Path)` (at launch: removes `<app>.previous` when it exists).
- [x] **Step 1: failing tests**:
  - `verify`: each failure reason, and Ok.
  - `swap_script` run for real with `/bin/sh` in a temp dir (directories stand in for the apps; the pid is one that
    has exited):
    - after it runs, `app` holds the new content, `app.previous` the old, and the new path is gone;
    - with `new_app` missing, the move fails, `app` is restored and `app.previous` is gone.
  - `clean_previous` removes `x.app.previous` and leaves `x.app`.
- [x] **Step 2: run** → FAIL.  **Step 3: implement.**  **Step 4: run** `cargo test`, clippy (both), fmt,
  `__ui-shots`, `scripts/bundle-mac.sh --app-only` → ok.
- [x] **Step 5: commit** `feat(m11e): install the update — download, check, swap and reopen`.

### Task 6: Merge and push (your word) — ✅ (2026-11-08)

After the whole-branch review: merge to `master` and push `master` to origin. Both only on your word.

### Task 7: The first release, v0.3.0 — ✅ (2026-11-08)

- [x] On your word, Claude tags `v0.3.0` on `master` and pushes the tag. Then it watches the Release run
  (`gh run watch`). If the run is red, the fix goes into Task 1's files with a new commit and a new patch version, so
  the tag is never moved.
- [x] Install 0.3.0 into `~/Applications` by Task 0's route, from the release's zip this time (the last manual
  install).

### Task 8: On screen — 👀 (you)

- [ ] On your word, release `v0.3.1`: a version bump only, plus its Progress Log line. Then work through note 57's
  checklist, items 4–12.

### Task 9: Docs

- [ ] Note 57 (status, Task 0's result, anything that changed while building). This plan (ticks). `40 - Roadmap.md`.
  The Plan Tracker. `60 - Progress Log.md`. `12 - Scope.md` (already updated at planning time). `STRUCTURE.md` (the
  2 modules, the workflow, the script). `README.md`: how to install (the zip from Releases) and that fuxx updates
  itself.
- [ ] Commit `docs(m11e): …`.
