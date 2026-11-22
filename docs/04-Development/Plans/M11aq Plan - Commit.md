# M11aq Commit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (this repo's build mode: the session
> builds every task itself; per-task gate `cargo test --bin fuxx`, both clippy runs, `cargo fmt --check`; screenshots
> and one whole-branch review only at the end). Steps use checkbox (`- [ ]`) syntax.

**Goal:** Commit the staged files from the Git tab, and let the workspace's agent write the message (✦).

**Architecture:** a new `git_commit` module does the git side (commit, what's in progress, the staged diff for the
agent) on real repos, off the UI thread. Providers gain a one-shot "write a commit message" probe run by
`agent_catalog::run_probe` (now cancellable). The Changes panel grows a commit line above its branch bar: a
`NameInput`, ✦ and ✓N, its draft per repo, its error line.

**Tech Stack:** Rust, GPUI (pinned rev), `git` via `git_branch::run_git*`, the agents' CLIs (`claude -p`, `agy -p`).

**Spec:** [[92 - Milestone 11aq - Commit]] (approved 2026-11-22: "Approved"). **Released in v0.12.3.**

## Global Constraints

- Never `--no-verify`, `--amend`, force, or a commit while a merge, rebase, cherry-pick or revert is in progress.
- Commit takes the staged files only (`git commit -m <message>`); the message as typed (trimmed).
- ✦ sends the **staged diff only**, capped at 24 KB, leaving out files that look like secrets (`.env*`, `*.pem`,
  `*.key`, `id_rsa*`, `*secret*`, `*credentials*`), and never commits.
- Provider-specific argv and parsing live in `src/providers/<agent>.rs` (memory: provider code in provider files).
- The field shows only while the repo has changes; quiet at rest (no fill, no border).
- Scope: [[12 - Scope]] M11aq line. Out: hunks, stash, PR status, conflicts, amend, undo, commit-and-push.

## Rulings (made while planning; the spec is the authority)

- **One-line field:** the app's one-line box (`NameInput`) is the field; the spec's "grows to four lines" waits for a
  multi-line box (the agent composer sends on Enter). ✦ asks for a summary line (≤ 72 chars). *Cost if wrong:* no
  commit body from the tab yet.
- **Show output inline:** fuxx has no "open text in a tab" yet; "Show output" expands a small scrolling block under the
  field (≤ 8 lines). *Cost if wrong:* a long hook log needs scrolling there.
- **✦ on Claude uses `--model haiku`** (quick and cheap for a summary); agy its default. *Cost if wrong:* a plainer
  message than the big model's.

## Review Focus

1. A pre-commit hook that runs long (a linter): the line says "Committing…", nothing freezes, the message is kept on
   failure.
2. The panel's repo changes (another tab followed) while a commit or ✦ runs: the answer lands on its own repo's draft,
   never the new one.
3. ✦ cancelled with Esc: the agent's process is killed; a late answer is dropped.
4. A message with quotes, newlines or a leading `-`: committed as written (one `-m` argument), no shell.
5. Nothing staged after all (staged elsewhere meanwhile): git's refusal shows as the error line, the draft kept.

---

### Task 1: `git_commit` — commit, in-progress, the staged diff

**Files:** Create `src/git_commit.rs`; modify `src/main.rs` (mod). Tests in the module, on real temporary repos
(`git_stage`'s test helpers' style).

**Interfaces — produces:**
- `pub fn commit(root: &Path, message: &str) -> Result<(), String>` — `git commit -m <message>`; Err holds git's
  whole output (stdout + stderr: a hook's log).
- `pub fn in_progress(root: &Path) -> Option<&'static str>` — `"a merge"`, `"a rebase"`, `"a cherry-pick"`,
  `"a revert"` from `git rev-parse --git-path` (`MERGE_HEAD`, `rebase-merge`, `rebase-apply`, `CHERRY_PICK_HEAD`,
  `REVERT_HEAD`).
- `pub struct StagedDiff { pub text: String, pub skipped: Vec<String>, pub cut: bool }` and
  `pub fn staged_diff(root: &Path, cap: usize) -> Result<StagedDiff, String>` — `git diff --cached --no-color
  --no-ext-diff` without secret-looking files (`skipped` names them), cut at `cap` bytes on a line end (`cut`).
- `pub fn is_secret(path: &str) -> bool`; `pub const DIFF_CAP: usize = 24 * 1024`.

- [ ] Write tests: `commit_takes_the_staged_files_only` (one staged, one not: the commit has one, the other still
  changed); `a_failing_hook_returns_its_output` (a `pre-commit` printing `nope` and exiting 1: Err contains `nope`,
  nothing committed); `a_merge_in_progress_is_named` (a conflicting merge: `Some("a merge")`; a clean repo: `None`);
  `the_staged_diff_leaves_secrets_out_and_is_capped` (`.env` staged: not in `text`, in `skipped`; a cap of 40 bytes:
  `cut`, text ≤ 40); `is_secret` cases (`.env`, `config/.env.local`, `id_rsa`, `a.pem` yes; `src/env.rs`, `README.md`
  no); `a_message_with_quotes_and_a_dash_is_kept` (`-x "q"\nline` committed as written).
- [ ] Run them: fail (no module). Implement. Run: pass. Gate. Commit `M11aq Task 1: git_commit — commit, in progress,
  the staged diff`.

### Task 2: The one-shot message probe, cancellable

**Files:** `src/providers/mod.rs`, `src/providers/claude.rs`, `src/providers/agy.rs`, `src/agent_catalog.rs`.

**Interfaces — produces:**
- `Provider::message_probe(&self, prompt: &str) -> Option<Probe>` (default `None`); Claude: `claude -p <prompt>
  --output-format text --model haiku --no-session-persistence`; agy: `agy -p <prompt>` (its text output form).
- `pub fn commit_message_prompt(diff: &StagedDiff) -> String` (in `providers/mod.rs`: asks for one imperative summary
  line ≤ 72 chars, only the line, no quotes or fences; says files were left out when `skipped`).
- `pub fn clean_message(raw: &str) -> Option<String>` — first non-empty line, fences/quotes/`Commit message:` stripped,
  trimmed; `None` when nothing is left.
- `pub fn on_path(command: &str, path_env: Option<&str>) -> bool`.
- `run_probe(probe, cwd, path_env, deadline, stop: &AtomicBool)` — a set `stop` kills its process group and returns
  `Err("stopped")` (existing callers pass a flag never set).

- [ ] Tests: each provider's argv; `clean_message` (fenced, quoted, prefixed, blank → None); the prompt names skipped
  files; `run_probe` with `stop` set early returns `stopped` quickly (an `sh -c 'sleep 5'` probe, as the existing
  `sh(...)` test helper).
- [ ] Fail, implement, pass, gate. Commit `M11aq Task 2: the one-shot message probe, cancellable`.

### Task 3: The commit line — field, ✓N, keys, the commit

**Files:** `src/changes_panel.rs` (state and render above `render_changes_bar`), `src/main.rs` (key bindings),
`src/ui_shots/changes.rs` (scenes).

**Interfaces:**
- Consumes `git_commit::{commit, in_progress}`.
- Produces `pub fn commit_blocked(staged: usize, message: &str, in_progress: Option<&str>) -> Option<String>` (pure:
  "Stage files to commit", "Write a message", "Finish the merge first" …; None = can commit).
- `ChangesPanel`: `drafts: HashMap<PathBuf, String>`, `commit_input: Option<Entity<NameInput>>`,
  `commit_busy: Option<(PathBuf, CommitWork)>` (`Committing | Writing(Arc<AtomicBool>)`),
  `commit_error: Option<(PathBuf, String /*output*/, bool /*shown*/)>`.

- [ ] Tests: `commit_blocked` cases; the draft follows the repo (a pure `swap_draft` on the map); an answer for another
  root lands on its own draft (Review Focus 2).
- [ ] Build: the line above the bar while there are changes: the input (placeholder "Message (⌘↵ to commit)"), ✓N
  (`header_button` style, the staged count, dim with `commit_blocked`'s words as its tooltip). ⌘↵ in the field and ✓
  commit off the UI thread ("Committing…" in the placeholder); success: draft cleared, changes refreshed (the bar's
  ↑N follows from the next sync); failure: one `ring_waiting`-coloured line "Commit failed · Show output", Show output
  expands the output inline (≤ 8 lines, scrolls). Esc leaves the field; ⌘⇧C focuses it (a `FocusCommit` action).
- [ ] Scenes `changes-commit` (at rest, typing, blocked tooltips) and `changes-commit-failed` (a hook's output shown):
  on a real temporary repo, checks by part and pixel. Gate. Commit `M11aq Task 3: the commit line`.

### Task 4: ✦ Write the message

**Files:** `src/changes_panel.rs`, `src/ui_shots/changes.rs`.

**Interfaces:** consumes Task 2's probe, `staged_diff`, `agent_catalog::run_probe` with a stop flag.
- Produces `pub fn message_agent(followed: Option<&'static str>, on_path: impl Fn(&str) -> bool) -> Option<&'static
  dyn Provider>` (pure: the followed agent tab's provider if it can run, else Claude, else agy).

- [ ] Tests: `message_agent` order; a late answer after Esc is dropped (state test on `ChangesPanel`).
- [ ] Build: ✦ (borderless, drawn as the app's ✦ text glyph) in the field, shown only when `message_agent` finds one.
  Press: `staged_diff`, then the probe off the UI thread (60 s deadline); "Writing…" in the placeholder; Esc sets the
  stop flag. The answer (`clean_message`) replaces the draft, selected; failure: a faint "Couldn't write a message",
  the draft untouched.
- [ ] Scene `changes-commit-write` (a scripted probe via a test provider hook: "Writing…", then the message in the
  field). Gate. Commit `M11aq Task 4: ✦ write the message`.

### Task 5: Finish — screenshots, review, docs

- [ ] Full `scripts/ui-shots.sh`; one whole-branch review on the most capable model; one fix wave; the full screenshot
  run again.
- [ ] Docs: note 92 "as built" and its checklist, Roadmap (M11aq built), Plan Tracker, Progress Log. Commit.
