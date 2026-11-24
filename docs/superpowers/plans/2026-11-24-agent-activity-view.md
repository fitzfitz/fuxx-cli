# Agent Activity View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** An agent tab shows what each step touched and how its checks went: a step with 3+ calls draws as file tracks with beads, a step with 1–2 calls as readable rows, and the box's folder tab carries a turn ledger (running call, changed files, checks) with a popover.

**Architecture:** One provider-neutral `Activity` per tool call, computed once by the provider's translator (start and finish) through shared pure parsers, sent as a new `Event::Activity` and stored on `Item::Tool`. A pure step model (`agent_steps`) and a pure ledger model (`agent_ledger`) are rebuilt only when rows change (cached on the pane's `Row`, like parsed markdown). The views draw from those caches: tracks inside today's tool group, and the ledger by extending `status_pill` plus a `BoxMenu` popover on `overlay_panel::shell`.

**Tech Stack:** Rust, GPUI (pinned git dep), the existing `ui_shots` harness. No new crates.

**Spec:** `docs/superpowers/specs/2026-11-24-agent-activity-view-design.md` (a chore under "Chores after M11ar" in `docs/04-Development/93 - Milestone 11ar - Subagents.md`, not a milestone).

## Global Constraints

- Agent-specific parsing stays in `src/providers/<agent>.rs`; generic code asks the activity model.
- No change to what agents run, no model calls, no new agent features.
- fuxx-cli is public: examples use fuxx-cli's own paths (`code/my-app`, `src/api.rs`); fixtures use `/tmp/fx/code/my-app/...` as the existing ones do.
- AGENTS.md "build for reuse, keep files lean, no duplication": extend the existing tool group, `overlay_panel::shell`, the diff view and status pill instead of copying them; new parsing lives in its own focused modules. `agent_tab.rs` (7 053 lines) and `agent_box.rs` get wiring only; drawing goes in new submodules.
- Gate as every chore: tests first, `cargo test --bin fuxx` (+ `--features ui-shots`), both clippy runs at baseline (14/29 and 18/33; no new warning on a changed line), `cargo fmt --check`, the full `scripts/ui-shots.sh` (once, in Task 6; no scenes per task).
- Native GPUI, fuxx's tokens and themes (light and dark), `crate::reduce_motion`, keyboard reachable. The running column has the spinning arc; nothing else moves (reduce motion: a still arc).
- New colour tokens are added to the existing contrast test (`agent_tab::tests::the_tab_s_text_meets_the_floor_in_every_theme`).
- Each finished tool call gets an `Activity`, computed once when the call finishes (and when history is read), and stored on the item. Drawing reads it; nothing is parsed per frame. Drawing must stay as cheap as today's rows.
- Commits are local only, never pushed, message style `Chore: …`.

## Review Focus

1. **A pipe that hides a failing exit code** (`cargo test 2>&1 | tail -20` exits 0): the check reads "failed" from the output; if `tail` cut the summary off, it reads Unknown, never green. Tests: Task 1 `the_output_wins_over_an_exit_code_a_pipe_hid`, `a_piped_check_with_no_summary_is_unknown_never_passed`; Task 2 `bash_reads_its_description_command_and_exit`.
2. **A step with 60+ calls:** the tracks show the latest 24 columns with "+36 earlier", the header still counts all 60, and the model is built once per change. Test: Task 3 `sixty_calls_show_the_latest_24_and_plus_36_earlier`.
3. **A failed Edit** ("String to replace not found"): a red-marked edit bead, "1 failed" in the header, and the file is never counted as changed (not in the ledger's "N changed"). Tests: Task 2 `a_failed_edit_touches_nothing_changed`; Task 3 `a_failed_call_beads_its_kind_with_a_mark_and_counts_in_the_header`; Task 5 `a_failed_edit_is_not_counted_changed`.
4. **Parallel subagents writing the same file** (one by absolute path, one by a relative `sed -i`): Main's ledger counts the file once with its +/− summed; each lane's ledger shows only its own. Test: Task 5 `parallel_subagents_editing_one_file_count_it_once`.
5. **A heredoc that only reads** (`python3 - <<'EOF' … open('src/api_client.rs').read() … EOF`): the file is "touched by a script", never "edited" or "changed", and appears apart in the popover. Tests: Task 1 `a_heredoc_that_only_reads_touches_by_script_never_changes`; Task 3 summary `"touched by a script"`; Task 5 `script_touched_files_are_listed_apart_and_never_counted`.

---

## The per-task gate (referred to as "the gate" below)

```bash
cargo test --bin fuxx
cargo test --bin fuxx --features ui-shots
cargo clippy --all-targets 2>&1 | tail -3                    # baseline 14/29
cargo clippy --all-targets --features ui-shots 2>&1 | tail -3  # baseline 18/33
cargo fmt --check
scripts/prune-target.sh
```

Expected: all tests pass; both clippy counts equal the baseline recorded in Task 1 Step 0 and no warning points at a changed line; fmt prints nothing.

---

### Task 1: The `Activity` model and the shared pure parsers

**Files:**
- Create: `src/agent_activity.rs` (types, titles, `finished`, `fallback`, `of`, `announces_failure`)
- Create: `src/agent_activity_command.rs` (command reading: segments, heredoc skip, verb table, file args, scope)
- Create: `src/agent_activity_checks.rs` (cargo / pytest / jest / vitest / go test output, exit precedence)
- Create: `src/fixtures/checks/` text fixtures: `cargo-test-failed.txt`, `cargo-test-failed-tail.txt`, `cargo-test-ok.txt` (two binaries: 12 + 2 passed), `cargo-build-e0425.txt`, `pytest-failed.txt`, `jest-failed.txt`, `vitest-failed.txt`, `go-test-failed.txt`
- Modify: `src/main.rs` (three `mod` lines), `src/command_name.rs` (`WRAPPERS` → `pub(crate)`, no other change)

**Interfaces:**
- Consumes: `crate::agent_detect::{is_assignment, unquote}`, `crate::command_name::{WRAPPERS, short_form}`, `crate::providers::ToolDetail`, `crate::agent_conversation::Item` (read only, for `of`).
- Produces (later tasks use exactly these):

```rust
// src/agent_activity.rs
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToolKind { Read, Edit, Delete, Move, Search, Execute, Fetch, Think, #[default] Other }
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum How {
    Read { lines: Option<(u32, u32)> },
    Changed { plus: Option<u32>, minus: Option<u32>, at: Option<u32> }, // `at`: only from a sed address
    ScriptTouched,
    Searched { matches: Option<u32> },
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Touch { pub path: String, pub how: How }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckWhat { Tests, Build, Lint }
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    Passed { n: Option<u32> },
    Failed { failed: Option<u32>, of: Option<u32>, first_error: Option<String> },
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Check { pub what: CheckWhat, pub scope: String, pub outcome: Outcome }
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Activity {
    pub kind: ToolKind,
    pub said: Option<String>,   // the agent's own words (Claude's Bash `description`, agy's `toolSummary`)
    pub derived: String,        // fuxx's verb + object ("Read api_client.rs 80–400")
    pub touches: Vec<Touch>,
    pub failed: Option<String>, // the call itself failed (not a check): its first error line
    pub not_run: bool,          // denied (stopped calls are read from the item: not running, ok None)
    pub check: Option<Check>,
    pub planned_failure: bool,
    pub exit: Option<i32>,
}
impl Activity {
    pub fn title(&self) -> &str;              // said, else derived
    pub fn running_title(&self) -> String;    // said, else the present tense ("Running tests")
    pub fn changed(&self) -> impl Iterator<Item = &Touch>; // Changed touches; none if failed or not_run
    pub fn real_failure(&self) -> bool;       // failed (not not_run), or a check Failed and !planned_failure
    pub fn check_failed(&self) -> bool;       // a check whose outcome is Failed
}
pub struct Finish<'a> {
    pub ok: bool, pub exit: Option<i32>, pub output: &'a str,
    pub error: Option<String>, // the provider's own first error line, when it knows one
    pub not_run: bool, pub piped: bool,
}
pub fn finished(start: Activity, end: Finish<'_>) -> Activity;
pub fn derived_words(kind: ToolKind, touches: &[Touch], check: Option<&Check>, object: &str) -> String;
pub fn fallback(name: &str, summary: &str, detail: Option<&ToolDetail>, ok: Option<bool>, output: Option<&str>) -> Activity;
pub fn of(item: &Item) -> Option<std::borrow::Cow<'_, Activity>>; // the stored one, else `fallback`; None: not a tool
pub fn announces_failure(text: &str) -> bool;

// src/agent_activity_command.rs
pub struct Reading {
    pub kind: ToolKind, pub touches: Vec<Touch>,
    pub check: Option<(CheckWhat, String)>, // what, normalised scope
    pub piped: bool,                        // the check's output goes into a pipe
    pub derived: String,
}
pub fn read_command(command: &str) -> Reading;
pub fn scope_of(words: &[String]) -> String;
pub fn looks_like_path(word: &str) -> bool;

// src/agent_activity_checks.rs
pub fn outcome(what: CheckWhat, output: &str, exit: Option<i32>, ok: bool, piped: bool) -> Outcome;
pub fn cargo(output: &str) -> Option<Outcome>;
pub fn pytest(output: &str) -> Option<Outcome>;
pub fn jest(output: &str) -> Option<Outcome>;   // jest and vitest
pub fn go_test(output: &str) -> Option<Outcome>;
pub fn strip_ansi(text: &str) -> std::borrow::Cow<'_, str>;
pub fn error_code(first_error: &str) -> Option<&str>; // "E0425"
```

- [ ] **Step 0: Branch and record the baseline**

```bash
git switch -c chore/agent-activity-view
cargo clippy --all-targets 2>&1 | tail -3
cargo clippy --all-targets --features ui-shots 2>&1 | tail -3
```

Expected: the counts 14/29 and 18/33. If they differ, write the real counts at the top of this task's commit message body and use them as the baseline.

- [ ] **Step 1: Capture the check fixtures**

`cargo-*.txt`: from a real run in a throwaway crate under `target/activity-fixtures/` (one passing and one failing test in `src/cli.rs`, one with a call to a missing `call_within`), paths rewritten to `/tmp/fx/code/my-app`; delete the crate after. pytest / jest / vitest / go: from a real run there if the tool is installed, else typed in the tool's real summary shapes below; each file's first line is a `#` comment saying which. No model calls.

- `pytest-failed.txt` ends `FAILED tests/test_api.py::test_get - assert 1 == 2` then `==== 1 failed, 5 passed in 0.12s ====`
- `jest-failed.txt` holds `● api › returns the user` and `Tests:       1 failed, 4 passed, 5 total`
- `vitest-failed.txt` holds ` FAIL  src/api.test.ts > returns the user` and ` Tests  1 failed | 4 passed (5)`
- `go-test-failed.txt` holds `--- FAIL: TestGet (0.00s)` and `FAIL\tcode/my-app/api\t0.01s`
- `cargo-test-failed-tail.txt` is the last 20 lines of `cargo-test-failed.txt` (it keeps `test result: FAILED. 2 passed; 1 failed; …`)

- [ ] **Step 2: Write the failing tests**

In `src/agent_activity_command.rs`:

```rust
fn touch(p: &str, how: How) -> Touch { Touch { path: p.into(), how } }
const CHANGED: How = How::Changed { plus: None, minus: None, at: None };

#[test]
fn a_heredoc_body_is_not_read_as_commands() {
    let r = read_command("python3 - <<'EOF'\nimport os\nos.remove('x.txt')\nEOF\ncargo test --bin fuxx cli:: 2>&1 | tail -20");
    assert_eq!(r.check, Some((CheckWhat::Tests, "cargo test --bin fuxx cli::".into())));
    assert!(r.piped);
    assert_eq!(r.kind, ToolKind::Execute);
    assert!(r.touches.iter().all(|t| !matches!(t.how, How::Changed { .. })));
}

#[test]
fn a_heredoc_that_only_reads_touches_by_script_never_changes() {
    let r = read_command("python3 - <<'EOF'\nsrc = open('src/api_client.rs').read()\nprint(src[:400])\nEOF");
    assert_eq!(r.touches, vec![touch("src/api_client.rs", How::ScriptTouched)]);
    assert_eq!(r.kind, ToolKind::Execute);
    assert_eq!(r.derived, "Ran a script on api_client.rs");
}

#[test]
fn a_cat_heredoc_writes_its_target_and_its_body_names_nothing() {
    let r = read_command("cat > notes.txt <<'EOF'\nsee src/api.rs\nEOF");
    assert_eq!(r.touches, vec![touch("notes.txt", CHANGED)]);
    assert_eq!(r.kind, ToolKind::Edit);
}

#[test]
fn verbs_map_to_reads_searches_changes_and_checks() {
    for (cmd, kind, touches) in [
        ("sed -n '80,400p' src/api_client.rs", ToolKind::Read, vec![touch("src/api_client.rs", How::Read { lines: Some((80, 400)) })]),
        ("cat src/cli.rs | head -40", ToolKind::Read, vec![touch("src/cli.rs", How::Read { lines: None })]),
        ("rg -n METHODS src/help_commands.rs", ToolKind::Search, vec![touch("src/help_commands.rs", How::Searched { matches: None })]),
        ("sed -i '543s/old/new/' src/api.rs", ToolKind::Edit, vec![touch("src/api.rs", How::Changed { plus: None, minus: None, at: Some(543) })]),
        ("echo done > out.txt 2>/dev/null", ToolKind::Edit, vec![touch("out.txt", CHANGED)]),
        ("rm src/old.rs", ToolKind::Delete, vec![touch("src/old.rs", CHANGED)]),
        ("mv src/a.rs src/b.rs", ToolKind::Move, vec![touch("src/a.rs", CHANGED), touch("src/b.rs", CHANGED)]),
    ] {
        let r = read_command(cmd);
        assert_eq!((r.kind, r.touches), (kind, touches), "{cmd}");
    }
    for (cmd, what, scope) in [
        ("cargo build 2>&1 | tail -5", CheckWhat::Build, "cargo build"),
        ("cargo clippy --all-targets -- -D warnings", CheckWhat::Lint, "cargo clippy --all-targets"),
        ("cargo fmt --check", CheckWhat::Lint, "cargo fmt --check"),
        ("pytest tests/test_api.py -q", CheckWhat::Tests, "pytest tests/test_api.py"),
        ("npx jest api", CheckWhat::Tests, "jest api"),
        ("npx vitest run api", CheckWhat::Tests, "vitest run api"),
        ("go test ./api/...", CheckWhat::Tests, "go test ./api/..."),
    ] {
        assert_eq!(read_command(cmd).check, Some((what, scope.to_string())), "{cmd}");
    }
}

#[test]
fn scope_drops_output_flags_and_keeps_runner_target_and_filter() {
    let w = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
    assert_eq!(scope_of(&w("cargo test -q --bin fuxx cli:: -- --nocapture")), "cargo test --bin fuxx cli::");
    assert_eq!(scope_of(&w("RUST_LOG=debug cargo test --bin fuxx cli::")), "cargo test --bin fuxx cli::");
    assert_ne!(scope_of(&w("cargo test --bin fuxx cli::")), scope_of(&w("cargo test --bin fuxx api::")));
}

#[test]
fn a_pipe_marks_the_check_piped_only_when_the_check_feeds_it() {
    assert!(read_command("cargo test 2>&1 | tail -20").piped);
    assert!(!read_command("cargo test 2>&1; echo done").piped);
}

#[test]
fn an_unclosed_quote_or_garbage_reads_as_a_plain_run() {
    let r = read_command("echo 'unclosed");
    assert_eq!(r.kind, ToolKind::Execute);
    assert!(r.touches.is_empty() && r.check.is_none());
    for odd in ["", "<<", "| |", "python3 - <<EOF"] { let _ = read_command(odd); } // no panic
}

#[test]
fn paths_look_like_files_not_flags_numbers_or_urls() {
    for yes in ["src/api.rs", "notes.txt", "./src/cli.rs"] { assert!(looks_like_path(yes), "{yes}"); }
    for no in ["--bin", "1.5", "https://example.com/a.js", "-n", "/dev/null"] { assert!(!looks_like_path(no), "{no}"); }
}
```

In `src/agent_activity_checks.rs` (fixtures via `include_str!("fixtures/checks/…")`):

```rust
#[test]
fn cargo_test_failures_sum_across_binaries() {
    assert_eq!(cargo(CARGO_TEST_FAILED), Some(Outcome::Failed {
        failed: Some(1), of: Some(3),
        first_error: Some("thread 'cli::tests::bad' panicked at src/cli.rs:42:9:".into()),
    }));
}
#[test]
fn cargo_test_passes_count_every_result_line() {
    assert_eq!(cargo(CARGO_TEST_OK), Some(Outcome::Passed { n: Some(14) }));
}
#[test]
fn a_compile_error_fails_with_its_first_error_and_code() {
    let first = "error[E0425]: cannot find function `call_within` in this scope";
    assert_eq!(cargo(CARGO_BUILD_E0425), Some(Outcome::Failed { failed: None, of: None, first_error: Some(first.into()) }));
    assert_eq!(error_code(first), Some("E0425"));
}
#[test]
fn pytest_jest_vitest_and_go_summaries() {
    assert!(matches!(pytest(PYTEST_FAILED), Some(Outcome::Failed { failed: Some(1), of: Some(6), first_error: Some(e) }) if e == "FAILED tests/test_api.py::test_get - assert 1 == 2"));
    assert_eq!(pytest("==== 6 passed in 0.10s ===="), Some(Outcome::Passed { n: Some(6) }));
    assert!(matches!(jest(JEST_FAILED), Some(Outcome::Failed { failed: Some(1), of: Some(5), first_error: Some(e) }) if e == "● api › returns the user"));
    assert!(matches!(jest(VITEST_FAILED), Some(Outcome::Failed { failed: Some(1), of: Some(5), .. })));
    assert!(matches!(go_test(GO_TEST_FAILED), Some(Outcome::Failed { failed: Some(1), of: None, first_error: Some(e) }) if e == "--- FAIL: TestGet (0.00s)"));
    assert_eq!(go_test("ok  \tcode/my-app/api\t0.01s\n"), Some(Outcome::Passed { n: None }));
}
#[test]
fn the_output_wins_over_an_exit_code_a_pipe_hid() {
    assert!(matches!(outcome(CheckWhat::Tests, CARGO_TEST_FAILED_TAIL, None, true, true),
        Outcome::Failed { failed: Some(1), of: Some(3), .. }));
}
#[test]
fn a_piped_check_with_no_summary_is_unknown_never_passed() {
    assert_eq!(outcome(CheckWhat::Tests, "   Compiling fuxx v0.13.0\n", None, true, true), Outcome::Unknown);
    assert_eq!(outcome(CheckWhat::Lint, "", Some(0), true, false), Outcome::Passed { n: None });
    assert_eq!(outcome(CheckWhat::Lint, "Diff in /tmp/fx/code/my-app/src/api.rs at line 3:\n", Some(1), false, false),
        Outcome::Failed { failed: None, of: None, first_error: Some("Diff in /tmp/fx/code/my-app/src/api.rs at line 3:".into()) });
}
#[test]
fn ansi_colours_are_ignored() {
    assert!(matches!(pytest("\u{1b}[31m==== 1 failed, 5 passed in 0.12s ====\u{1b}[0m"), Some(Outcome::Failed { failed: Some(1), of: Some(6), .. })));
}
```

In `src/agent_activity.rs`:

```rust
#[test]
fn the_agents_words_lead_the_title() {
    let mut a = Activity { derived: "Ran tests cli::".into(), said: Some("Run the CLI tests".into()), ..Default::default() };
    assert_eq!(a.title(), "Run the CLI tests");
    a.said = None;
    assert_eq!(a.title(), "Ran tests cli::");
}
#[test]
fn derived_words_say_verb_and_object() {
    let t = |p: &str, how| vec![Touch { path: p.into(), how }];
    assert_eq!(derived_words(ToolKind::Read, &t("/tmp/fx/code/my-app/src/api_client.rs", How::Read { lines: Some((80, 400)) }), None, ""), "Read api_client.rs 80–400");
    assert_eq!(derived_words(ToolKind::Edit, &t("src/api.rs", How::Changed { plus: None, minus: None, at: Some(543) }), None, ""), "Edited api.rs:543");
    let tests = Check { what: CheckWhat::Tests, scope: "cargo test --bin fuxx cli::".into(), outcome: Outcome::Unknown };
    assert_eq!(derived_words(ToolKind::Execute, &[], Some(&tests), ""), "Ran tests cli::");
    let two = [Touch { path: "src/a.rs".into(), how: How::Searched { matches: None } }, Touch { path: "src/b.rs".into(), how: How::Searched { matches: None } }];
    assert_eq!(derived_words(ToolKind::Search, &two, None, "METHODS"), "Searched 2 files for METHODS");
    assert_eq!(derived_words(ToolKind::Fetch, &[], None, "https://docs.rs/gpui"), "Fetched docs.rs");
}
#[test]
fn running_titles_use_the_present() {
    let tests = Check { what: CheckWhat::Tests, scope: "cargo test".into(), outcome: Outcome::Unknown };
    assert_eq!(Activity { kind: ToolKind::Execute, check: Some(tests), ..Default::default() }.running_title(), "Running tests");
    let read = vec![Touch { path: "src/api.rs".into(), how: How::Read { lines: None } }];
    assert_eq!(Activity { kind: ToolKind::Read, touches: read, ..Default::default() }.running_title(), "Reading api.rs");
}
#[test]
fn a_failed_call_changes_nothing() {
    let edit = vec![Touch { path: "src/api.rs".into(), how: How::Changed { plus: Some(1), minus: Some(0), at: None } }];
    let mut a = Activity { kind: ToolKind::Edit, touches: edit, ..Default::default() };
    assert_eq!(a.changed().count(), 1);
    a.failed = Some("String to replace not found in file.".into());
    assert_eq!(a.changed().count(), 0);
    assert!(a.real_failure());
    a.not_run = true;
    assert!(!a.real_failure());
}
#[test]
fn failure_announcements_are_read_from_the_agents_words() {
    for yes in ["Write the failing test first (red first).", "Run it (red)", "Run the failing test first",
                "This should fail", "I expect the new test to fail", "TDD: add the test"] {
        assert!(announces_failure(yes), "{yes}");
    }
    for no in ["Run the tests", "Fix the failing build", "Expected output", "Read the red theme"] {
        assert!(!announces_failure(no), "{no}");
    }
}
#[test]
fn finishing_reads_a_check_s_outcome_and_a_plain_command_s_exit() {
    let check = Check { what: CheckWhat::Tests, scope: "cargo test".into(), outcome: Outcome::Unknown };
    let start = Activity { kind: ToolKind::Execute, check: Some(check), ..Default::default() };
    let done = finished(start, Finish { ok: true, exit: None, output: CARGO_TEST_FAILED_TAIL, error: None, not_run: false, piped: true });
    assert!(done.check_failed() && done.failed.is_none());
    let plain = Activity { kind: ToolKind::Execute, derived: "Ran ls".into(), ..Default::default() };
    let done = finished(plain, Finish { ok: false, exit: Some(2), output: "ls: nope: No such file or directory", error: None, not_run: false, piped: false });
    assert_eq!((done.failed.as_deref(), done.exit), (Some("ls: nope: No such file or directory"), Some(2)));
}
#[test]
fn a_provider_that_sends_none_gets_a_fallback() {
    let diff = ToolDetail::Diff { path: "/tmp/fx/code/my-app/src/api.rs".into(), unified: "@@ -1,1 +1,2 @@\n-a\n+b\n+c\n".into() };
    let a = fallback("Edit", "api.rs", Some(&diff), Some(true), None);
    assert_eq!(a.kind, ToolKind::Edit);
    assert_eq!(a.changed().count(), 1);
    let cmd = ToolDetail::Command("cargo test".into());
    assert_eq!(fallback("Bash", "cargo test", Some(&cmd), Some(true), Some("test result: ok. 2 passed; 0 failed")).check.map(|c| c.outcome), Some(Outcome::Passed { n: Some(2) }));
}
```

- [ ] **Step 3: Run them and see them fail**

Run: `cargo test --bin fuxx agent_activity`
Expected: compile errors (the modules and names don't exist yet).

- [ ] **Step 4: Implement the three modules**

Pinned behaviour the tests don't fully fix:
- **Segments:** split on `&&`, `||`, `;`, `|`, newlines, outside quotes; a `<<WORD` / `<<'WORD'` / `<<-WORD` heredoc's body (up to the line equal to `WORD`, trimmed for `<<-`) is removed from the command text before splitting. Leading assignments (`is_assignment`) and `command_name::WRAPPERS` are skipped. Do not refactor `command_name::first_command` (it stops at the first command and knows no heredocs).
- **Heredoc paths:** only when the heredoc feeds an interpreter (`python`, `python3`, `node`, `ruby`, `perl`, `sh`, `bash`, `zsh`, `deno`, `bun`; also `python3 -c '…'`): quoted string literals in the body that `looks_like_path` become `ScriptTouched`.
- **Verb table:** read = `cat`, `head`, `tail`, `less`, `wc`, `nl`, `bat`, `sed -n` (address `A,Bp` → `lines`); search = `rg`, `grep`, `ag`, `find`, `fd`, `ls` (`find`/`fd`/`ls` touch no file); change = `sed -i` (address `N` → `at`), `>`/`>>` targets (not `/dev/null`, not `&N`), `tee` args, `cp` dest, `touch`; delete = `rm`; move = `mv` (both paths); check = `cargo test|build|check|clippy|fmt --check` (`check` → Build, `clippy`/`fmt` → Lint), `pytest`, `python -m pytest`, `jest`, `vitest`, `npm test`, `npm run test`, `go test|build|vet` (`build` → Build, `vet` → Lint); `npx`/`pnpm`/`yarn` prefixes skipped for the runner. Command kind precedence: check → Execute; else a delete/move/change → Delete/Move/Edit; else an interpreter script → Execute; else search → Search; else read → Read; else Execute.
- **Scope:** runner words (program + subcommand, `run` kept for vitest), target flags with their values (`--bin X`, `-p X`, `--package X`, `--lib`, `--test X`, `--features X`, `--all-targets`, `--check`), and positional filters; dropped: output flags (`-q`, `--quiet`, `-v`, `--color*`, `--no-fail-fast`), everything after a bare `--`, redirects.
- **Checks precedence (`outcome`):** a recognised summary in the output (ANSI stripped) wins; else exit `Some(0)` → `Passed{n: None}`, `Some(_)` → `Failed{first_error: first non-empty line}`; else (no exit) `piped` → `Unknown`; else `ok` → `Passed{n: None}`, `!ok` → `Failed{first_error: first non-empty line}`. cargo's first error: the first `error[E…]:` / `error:` line, else the first non-blank line after the first `---- … stdout ----`.
- **`finished`:** with a check, fills `check.outcome` via `outcome` and leaves `failed` None; without one, `failed = end.error.or(first non-empty output line)` when `!ok` or `exit` is non-zero; sets `exit`, `not_run`. `planned_failure` stays false here (the conversation sets it, Task 2).
- **`announces_failure`:** case-insensitive; matches "red first", "(red)", "failing first", "failing test first", "should fail", "expect" followed later in the same sentence by "to fail", and the word "TDD".

- [ ] **Step 5: Run them and see them pass**

Run: `cargo test --bin fuxx agent_activity`
Expected: PASS.

- [ ] **Step 6: Run the gate**

- [ ] **Step 7: Commit**

```bash
git add src/agent_activity.rs src/agent_activity_command.rs src/agent_activity_checks.rs src/fixtures/checks src/main.rs src/command_name.rs
git commit -m "Chore: the agent activity model — one provider-neutral Activity per tool call, with shared command and check readers"
```

---

### Task 2: Providers fill `Activity`; it is stored on the tool item, with live times

**Files:**
- Modify: `src/providers/mod.rs` (new `Event::Activity`)
- Modify: `src/providers/claude.rs` (start/finish activities, `exit_code`, `error_line`, Grep touches, denied ids)
- Modify: `src/providers/agy.rs` (`command_output` returns the code, start/finish activities, `toolSummary` in lanes, lanes' exit header)
- Create: `src/providers/fixtures/claude-activity.jsonl` (synthetic, real-shaped; first line a `{"type":"fuxx_note","synthetic":true,…}` record like the resume fixture's)
- Modify: `src/agent_conversation.rs` (`Item::Tool.activity`, `Event::Activity`, planned failure, `narration_before`, `live_from`)
- Modify: `src/agent_tab.rs` (`tool_started` → `tool_times: ToolTimes`, finish recorded; `fingerprint` hashes the activity), `src/agent_rows.rs` (the elapsed read moves to `tool_times`)
- Modify: `src/agent_chat.rs`, `src/agent_tool_modal.rs` (their test helpers' `Item::Tool` literals gain `activity: None`)

**Interfaces:**
- Consumes: Task 1's `Activity`, `Finish`, `finished`, `derived_words`, `read_command`, `announces_failure`.
- Produces:

```rust
// src/providers/mod.rs — sent right after the ToolStarted (the call as planned) and right after
// the ToolFinished (the final one) of the same id; history replays the same pairs.
Event::Activity { id: String, activity: crate::agent_activity::Activity }

// src/agent_conversation.rs
Item::Tool { /* …as today… */ activity: Option<Activity> }
impl Conversation {
    pub fn narration_before(&self, ix: usize) -> Option<&str>; // the agent text just before the run holding item ix (same parent)
    pub fn live_from(&self) -> usize;                          // items.len() at the end of `past()` (0: none restored)
}

// src/agent_steps.rs is Task 3's; the alias lives there, Task 2 creates the file with only:
pub type ToolTimes = std::collections::HashMap<String, (std::time::Instant, Option<std::time::Instant>)>;

// src/providers/claude.rs (private)
fn activity_at_start(name: &str, input: &Value) -> Activity;
fn exit_code(text: &str) -> Option<i32>;      // a failing Bash result's first line `Exit code N`
fn error_line(text: &str) -> Option<String>;  // first non-empty line, `<tool_use_error>` tags and the exit line stripped
fn grep_touches(input: &Value, output: &str) -> Vec<Touch>;

// src/providers/agy.rs (private)
fn command_output(output: &str) -> (String, Option<i32>);   // was (String, Option<bool>)
fn activity_at_start(name: &str, params: &Value, said: Option<String>) -> Activity;
```

- [ ] **Step 1: Write the fixture and the failing tests**

`claude-activity.jsonl` (stream lines in Claude's real `assistant` / `user` shapes, ids `toolu_act_1…8`, under `/tmp/fx/code/my-app`): an agent text "Now implement the CLI side."; Read `src/api_client.rs` offset 80 limit 321; Bash (description "Look at the client") `python3 - <<'EOF'` reading `src/api_client.rs`; Edit `src/api.rs` (old 1 line, new 2 lines); Grep `METHODS` in content mode with 3 lines from `src/help_commands.rs`; Bash (description "Run the CLI tests") `cargo test --bin fuxx cli:: 2>&1 | tail -20`, ok, output the failing tail; Edit `src/cli.rs` with an `is_error` result `<tool_use_error>String to replace not found in file.\nString: fn old()</tool_use_error>`; Bash `ls nope` failing with `Exit code 2\nls: nope: No such file or directory`; a text "Write the failing test first (red first)." then Bash `cargo test --bin fuxx api::` failing with `Exit code 101` and a cargo failure; then `result`.

In `src/providers/claude.rs` tests:

```rust
const ACTIVITY: &str = include_str!("fixtures/claude-activity.jsonl");
fn activity_of(ev: &[Event], id: &str) -> Activity {
    ev.iter().rev().find_map(|e| match e { Event::Activity { id: i, activity } if i == id => Some(activity.clone()), _ => None }).expect(id)
}

#[test]
fn the_activity_fixture_says_it_is_synthetic() { assert!(ACTIVITY.lines().next().unwrap().contains("\"synthetic\":true")); }

#[test]
fn every_tool_gets_an_activity_after_its_start_and_after_its_finish() {
    let ev = decode_all(&CLAUDE, ACTIVITY);
    for id in (1..=8).map(|n| format!("toolu_act_{n}")) {
        let at = |pred: &dyn Fn(&Event) -> bool| ev.iter().position(pred).expect(&id);
        let started = at(&|e| matches!(e, Event::ToolStarted { id: i, .. } if *i == id));
        let finished = at(&|e| matches!(e, Event::ToolFinished { id: i, .. } if *i == id));
        assert!(matches!(&ev[started + 1], Event::Activity { id: i, .. } if *i == id));
        assert!(matches!(&ev[finished + 1], Event::Activity { id: i, .. } if *i == id));
    }
}

#[test]
fn bash_reads_its_description_command_and_exit() {
    let a = activity_of(&decode_all(&CLAUDE, ACTIVITY), "toolu_act_5");
    assert_eq!(a.said.as_deref(), Some("Run the CLI tests"));
    assert!(matches!(a.check, Some(Check { what: CheckWhat::Tests, outcome: Outcome::Failed { failed: Some(1), of: Some(3), .. }, .. })));
    assert_eq!(exit_code("Exit code 2\nls: nope: No such file or directory"), Some(2));
    assert_eq!(exit_code("total 8"), None);
    let ls = activity_of(&decode_all(&CLAUDE, ACTIVITY), "toolu_act_7");
    assert_eq!((ls.exit, ls.failed.as_deref()), (Some(2), Some("ls: nope: No such file or directory")));
}

#[test]
fn read_edit_write_grep_and_glob_fill_touches() {
    let ev = decode_all(&CLAUDE, ACTIVITY);
    assert_eq!(activity_of(&ev, "toolu_act_1").touches[0].how, How::Read { lines: Some((80, 400)) });
    assert_eq!(activity_of(&ev, "toolu_act_2").touches[0].how, How::ScriptTouched);
    assert_eq!(activity_of(&ev, "toolu_act_3").touches[0].how, How::Changed { plus: Some(2), minus: Some(1), at: None });
    assert_eq!(activity_of(&ev, "toolu_act_4").touches, vec![Touch { path: "src/help_commands.rs".into(), how: How::Searched { matches: Some(3) } }]);
    let write = activity_at_start("Write", &json!({"file_path": "/tmp/fx/code/my-app/src/link_route.rs", "content": "a\n".repeat(41)}));
    assert_eq!(write.touches[0].how, How::Changed { plus: Some(41), minus: None, at: None });
    let glob = activity_at_start("Glob", &json!({"pattern": "**/*.rs"}));
    assert_eq!((glob.kind, glob.touches.len(), glob.derived.as_str()), (ToolKind::Search, 0, "Listed **/*.rs"));
    let files = grep_touches(&json!({"pattern": "METHODS", "output_mode": "files_with_matches"}), "Found 2 files\nsrc/a.rs\nsrc/b.rs");
    assert_eq!(files.len(), 2);
}

#[test]
fn a_failed_edit_touches_nothing_changed() {
    let a = activity_of(&decode_all(&CLAUDE, ACTIVITY), "toolu_act_6");
    assert_eq!(a.failed.as_deref(), Some("String to replace not found in file."));
    assert_eq!(a.changed().count(), 0);
    assert!(a.real_failure());
}

#[test]
fn a_denied_call_is_not_run() {
    // claude-permission-deny.jsonl: decode up to its can_use_tool line, encode Action::Deny for it
    // (as `deny_fixture_maps_the_denial_to_a_failed_tool` does), then decode the rest.
    let a = activity_of(&events, &write_call_id);
    assert!(a.not_run && !a.real_failure());
    assert_eq!(a.changed().count(), 0);
}

#[test]
fn history_rebuilds_the_same_activities() {
    let transcript: String = ACTIVITY.lines().filter(|l| l.contains("\"type\": \"assistant\"") || l.contains("\"type\": \"user\"")).collect::<Vec<_>>().join("\n");
    let live = decode_all(&CLAUDE, ACTIVITY);
    let past: Vec<Event> = transcript_events(&transcript).into_iter().filter_map(|p| match p { Past::Event(e) => Some(e), _ => None }).collect();
    for n in 1..=8 { let id = format!("toolu_act_{n}"); assert_eq!(activity_of(&live, &id), activity_of(&past, &id), "{id}"); }
}
```

In `src/providers/agy.rs` tests:

```rust
#[test]
fn agy_command_exit_is_the_header_s_code() {
    assert_eq!(command_output("Created At: x\nCompleted At: x\nCommand exited with code 1.\n\nCommand output:\nboom\n"), ("boom\n".into(), Some(1)));
    assert_eq!(command_output("plain"), ("plain".into(), None));
}
#[test]
fn agy_steps_fill_activities_without_counts() {
    // agy-tools.jsonl through decode_all
    // read_file → How::Read { lines: None }; edit_file / write_file → How::Changed { plus: None, minus: None, at: None };
    // run_command → read through read_command; every Main activity has said == None.
}
#[test]
fn an_agy_lane_s_tool_summary_is_its_words() {
    // agy-ask-transcript.jsonl's PLANNER_RESPONSE line through decode_followed
    assert_eq!(activity.said.as_deref(), Some("List workspace directory contents"));
}
#[test]
fn an_agy_lane_s_failing_command_is_not_ok() {
    // a GENERIC line whose content holds "Command exited with code 1." after a run_command call
    assert!(matches!(finished_event, Event::ToolFinished { ok: false, .. }));
    assert_eq!(activity.exit, Some(1));
}
```

In `src/agent_conversation.rs` tests:

```rust
#[test]
fn an_activity_is_stored_on_its_tool_and_replaced_at_its_finish() {
    // ToolStarted, Activity{derived: "Reading api.rs"}, ToolFinished, Activity{derived: "Read api.rs"}
    assert_eq!(tool_activity(&c, "t1").unwrap().derived, "Read api.rs");
}
#[test]
fn a_failure_the_narration_announced_is_planned() {
    // Agent "Write the failing test first (red first)." then a tool whose final Activity has a Failed check
    assert!(tool_activity(&c, "t1").unwrap().planned_failure);
    // the same without the narration: false; with said = "Run it, it should fail" and no narration: true
}
#[test]
fn narration_is_the_text_just_before_the_run() {
    // [You, Agent "A", Tool t1, Tool t2] → narration_before(ix of t2) == Some("A")
    // [Agent "A", You, Tool t1] → narration_before(ix of t1) == None
    // a subagent's tool reads only its own lane's text (same parent)
}
#[test]
fn live_from_is_where_the_history_ends() {
    // past(two items) → live_from() == items().len(); a fresh conversation → 0
}
```

In `src/agent_tab.rs` tests:

```rust
#[test]
fn a_tool_s_print_changes_with_its_activity() {
    // two Item::Tool equal but for activity: fingerprint differs
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --bin fuxx -- providers:: agent_conversation:: a_tool_s_print`
Expected: compile errors (`Event::Activity`, `activity` field, helpers missing).

- [ ] **Step 3: Implement**

- `Event::Activity` in `providers/mod.rs`. Every translator that emits a `ToolStarted` emits `Activity` right after it, and right after the `ToolFinished` of the same id.
- **Claude:** `ClaudeTranslator` keeps `calls: HashMap<String, (Activity, Option<String>)>` (the start activity and the Bash command) and `denied: HashSet<String>` (the `tool_use_id` of each permission it encodes a Deny for: store `req["tool_use_id"]` with the `pending` entry). `user()` becomes `&mut self` and builds the final one with `finished(start, Finish { ok, exit: exit_code(&text), output: &text, error: error_line(&text).filter(|_| !ok), not_run: denied.contains(id), piped: read_command(cmd).piped })`. `activity_at_start`: Read (`file_path`, `offset`, `limit` → `lines = (offset, offset + limit - 1)`), Edit / MultiEdit (plus = new lines, minus = old lines, summed), Write (plus = content lines), NotebookEdit (Changed), Grep (Search; touches only at finish from `grep_touches`, else `path` when it `looks_like_path`), Glob (Search, `"Listed {pattern}"`), WebFetch / WebSearch (Fetch), Bash (`description` → `said`, `read_command(command)`), Task / Agent / SendMessage (Other: they are lane and passed-on rows). Times are not read here.
- **agy:** `command_output` returns `Option<i32>`; `ok` stays "no error and exit 0". `tool()` and `decode_followed` keep `started: HashMap<String, (Activity, Option<String>)>`; `activity_at_start`: `read_file` (Read, no lines), `edit_file` / `write_file` (Changed, no counts), `run_command` (`read_command(CommandLine)`), other (Other). `said` only from a lane's `args["toolSummary"]` (Main's stream has none). A lane's `GENERIC` result reads the exit header through `command_output` too: `ok = exit != Some(non-zero)`.
- **Conversation:** `Item::Tool` gains `activity: Option<Activity>` (None at `ToolStarted`). `Event::Activity` sets it on the tool with that id; when that tool is finished and `activity.check_failed()`, it sets `planned_failure = activity.said.as_deref().is_some_and(announces_failure) || self.narration_before(ix).is_some_and(announces_failure)`. `past()` records `live_from`; `Cleared` resets it to 0.
- **Times:** rename `AgentTab::tool_started` to `tool_times: ToolTimes`; `Event::ToolStarted` inserts `(now, None)`, `Event::ToolFinished` sets the finish; `Cleared` clears it. History never enters it (it goes through `past()`), so restored calls have no times. The running row's elapsed in `agent_rows.rs` reads `tool_times`.
- `fingerprint` hashes `activity` for `Item::Tool`.

- [ ] **Step 4: Run them and see them pass**

Run: `cargo test --bin fuxx -- providers:: agent_conversation:: a_tool_s_print`
Expected: PASS, and the existing `tools_fixture_pairs_every_start_with_a_finish`, `a_failing_command_finishes_not_ok` still pass.

- [ ] **Step 5: Run the gate**

- [ ] **Step 6: Commit**

```bash
git add src/providers src/agent_conversation.rs src/agent_tab.rs src/agent_rows.rs src/agent_chat.rs src/agent_tool_modal.rs src/agent_steps.rs src/main.rs
git commit -m "Chore: Claude and agy fill each tool call's activity — descriptions, files, exit codes, check outcomes, planned failures — kept on the item, with live start and finish times"
```

---

### Task 3: The step model (pure): groups, tracks, summaries, header, open failures, row wording

**Files:**
- Modify: `src/agent_steps.rs` (created in Task 2 with `ToolTimes`; all of this task's model)
- Modify: `src/agent_chat.rs` (`group_shape_at` takes who stands alone; `group_shape` / `row_print_at` callers pass it)
- Modify: `src/agent_rows.rs` (the `group_shape_at` call passes `stands_alone`)

**Interfaces:**
- Consumes: Task 1 types and `Activity` methods, `agent_activity::of`; Task 2's `Item::Tool.activity`, `ToolTimes`, `Conversation::{lane, passed_on}`; `crate::agent_chat::Shape`; `crate::agent_lanes::clock` (`1:52`).
- Produces:

```rust
pub const TRACKS_FROM: usize = 3;   // a step of 3+ calls draws as tracks
pub const LATEST: usize = 24;       // columns shown
pub const MAX_TRACKS: usize = 6;    // more file tracks: the top 5, then "+N files"

pub fn stands_alone(conversation: &Conversation, item: &Item) -> bool; // a lane's row or a "Passed on" row

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Role { Row(Shape), Tracks(Vec<usize>), Inside } // Inside: a call drawn by its step's first row
pub fn role_at(items: &[Item], rows: &[usize], k: usize, alone: &dyn Fn(&Item) -> bool, fits: bool) -> Option<Role>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyph { Read, Search, Edit, Script, Passed, Failed, Planned, Check, Running }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Bead { pub glyph: Glyph, pub failed: bool, pub dim: bool } // failed: the small red ✕; dim: not run
pub struct Column { pub ix: usize, pub id: String, pub tip: String, pub running: bool }
pub struct Track { pub label: String, pub path: String, pub beads: Vec<(usize, Bead)>, pub summary: String }
pub struct Tracks {
    pub counts: String,                              // "8 actions · 5 files · 1 failed"
    pub span: Option<(Instant, Option<Instant>)>,    // first start, last finish (None while running); None: a time unknown
    pub columns: Vec<Column>,                        // every call, time order
    pub earlier: usize,                              // columns.len() - LATEST when more, else 0
    pub checks: Option<Track>,
    pub files: Vec<Track>,                           // all, ordered
    pub first_error: Option<String>,                 // the step's latest real check failure still open
}
impl Tracks {
    pub fn shown_columns(&self, all: bool) -> std::ops::Range<usize>;
    pub fn shown_files(&self, all: bool) -> (&[Track], usize);  // (tracks, the "+N files" count)
}
pub fn tracks_of(items: &[Item], calls: &[usize], open: &HashSet<usize>, times: &ToolTimes, cwd: &Path) -> Tracks;
pub fn header(counts: &str, took: Option<Duration>) -> String;
pub fn counts(actions: usize, files: usize, failed: usize) -> String;
pub fn file_summary(touches: &[(&Touch, bool)]) -> String;   // bool: that call failed
pub fn checks_summary(runs: &[&Activity]) -> String;
pub fn file_key(path: &str, cwd: &Path) -> PathBuf;
pub fn turn_start(items: &[Item]) -> usize;                   // index after the latest You (0: none)
pub fn open_failures(items: &[Item], from: usize) -> HashSet<usize>;
pub fn outcome_phrase(a: &Activity) -> Option<String>;        // "2 passed", "build failed · E0425", "red, as planned", "1 of 3 failed", "failed"
pub struct RowWords { pub title: String, pub trace: Option<String>, pub chips: Vec<String> }
pub fn row_words(item: &Item) -> RowWords;
pub fn column_tip(a: &Activity, took: Option<Duration>) -> String;
pub fn step_print(items: &[Item], calls: &[usize], open: &HashSet<usize>, fits: bool) -> u64;

// src/agent_chat.rs (changed signature)
pub fn group_shape_at(items: &[Item], rows: &[usize], k: usize, alone: &dyn Fn(&Item) -> bool) -> Option<Shape>;
```

- [ ] **Step 1: Write the failing tests** (in `src/agent_steps.rs`; helpers `read(id, path)`, `edit(id, path)`, `run_check(id, what, scope, outcome)`, `lane_row(id)` build `Item::Tool`s with activities; `CWD = "/tmp/fx/code/my-app"`)

```rust
#[test]
fn three_plain_calls_are_tracks_two_are_rows() {
    let two = [read("a", "src/a.rs"), read("b", "src/b.rs")];
    let none = |_: &Item| false;
    assert_eq!(role_at(&two, &[0, 1], 0, &none, true), Some(Role::Row(Shape::First)));
    let three = [read("a", "src/a.rs"), read("b", "src/b.rs"), edit("c", "src/c.rs")];
    assert_eq!(role_at(&three, &[0, 1, 2], 0, &none, true), Some(Role::Tracks(vec![0, 1, 2])));
    assert_eq!(role_at(&three, &[0, 1, 2], 2, &none, true), Some(Role::Inside));
    assert_eq!(role_at(&three, &[0, 1, 2], 1, &none, false), Some(Role::Row(Shape::Middle))); // narrow
}
#[test]
fn lane_and_passed_on_rows_break_a_track_group() {
    // [tool, tool, lane row, tool, tool, tool], alone = the lane row
    // roles: Row(First), Row(Last), Row(Solo), Tracks([3, 4, 5]), Inside, Inside
    // and group_shape_at gives the lane row Shape::Solo
}
#[test]
fn files_order_changed_first_then_read() {
    // read a, read b, edit c, search d, edit a → labels ["c.rs", "a.rs", "b.rs", "d.rs"]
}
#[test]
fn a_call_touching_two_files_beads_both_in_its_column() {
    // mv src/a.rs src/b.rs as column 0 → both tracks hold (0, Bead { glyph: Glyph::Edit, .. })
}
#[test]
fn more_than_six_files_show_five_and_plus_n() {
    // 8 files: shown_files(false) → 5 tracks and 3; shown_files(true) → 8 and 0; 6 files → 6 and 0
}
#[test]
fn sixty_calls_show_the_latest_24_and_plus_36_earlier() {
    // 60 reads cycling src/a.rs, src/b.rs, src/c.rs
    assert_eq!(t.columns.len(), 60);
    assert_eq!(t.earlier, 36);
    assert_eq!(t.shown_columns(false), 36..60);
    assert_eq!(t.shown_columns(true), 0..60);
    assert_eq!(t.counts, "60 actions · 3 files");
}
#[test]
fn file_summaries() {
    let ch = |at, p, m| Touch { path: "src/api.rs".into(), how: How::Changed { plus: p, minus: m, at } };
    assert_eq!(file_summary(&[(&ch(Some(543), Some(1), Some(0)), false)]), "edited at :543 · +1 −0");
    // read 80–400 + read 78–120 + two script touches → "read 80–400, 78–120 · script 2×"
    // one script touch alone → "touched by a script"
    // searched with 3 matches → "searched · 3 matches"
    // only a failed edit → "edit failed"
}
#[test]
fn checks_summaries() {
    // [Passed{2}] → "2 passed"
    // [Passed{2}, Failed{first_error "error[E0425]: …"}] → "2 passed → build failed E0425"
    // [planned Failed{1 of 3}] → "red, as planned"
    // [planned Failed, Passed{2}] (same kind and scope) → "red → green · 2 passed"
    // [Failed{1 of 3}] → "1 of 3 failed"
}
#[test]
fn header_counts_failed_calls_and_time_when_known() {
    assert_eq!(header(&counts(8, 5, 0), Some(Duration::from_secs(112))), "8 actions · 5 files · 1:52");
    assert_eq!(header(&counts(8, 5, 1), None), "8 actions · 5 files · 1 failed");
}
#[test]
fn a_failed_call_beads_its_kind_with_a_mark_and_counts_in_the_header() {
    // read a, failed edit b, read c → b's track bead (1, Bead { glyph: Edit, failed: true, dim: false }),
    // its summary "edit failed", counts "3 actions · 3 files · 1 failed"
    // a denied call (not_run) and a stopped one (running false, ok None): dim, not counted failed
}
#[test]
fn the_running_call_beads_an_arc_on_its_tracks() {
    // a running Edit of src/api.rs → its column running == true, bead glyph Running on api.rs's track
}
#[test]
fn a_real_failure_stays_until_the_same_kind_and_scope_passes() {
    // tests "cargo test cli::" fail (ix 1), lint pass, tests "cargo test api::" pass → open == {1}
    // then tests "cargo test cli::" pass → open is empty
}
#[test]
fn a_planned_failure_is_never_open() {
    // tests "cargo test cli::" fail with planned_failure = true, nothing after → open_failures(…, 0) is empty
}
#[test]
fn open_failures_count_only_from_the_turn_start() {
    // a failure before the latest You is not open; turn_start == index after that You
}
#[test]
fn file_keys_join_relative_paths_to_the_folder() {
    let cwd = Path::new("/tmp/fx/code/my-app");
    assert_eq!(file_key("src/api.rs", cwd), file_key("/tmp/fx/code/my-app/src/api.rs", cwd));
    assert_eq!(file_key("./src/api.rs", cwd), file_key("src/api.rs", cwd));
}
#[test]
fn row_words_lead_with_the_description_and_trace_the_command() {
    // Bash said "Run the CLI tests", command "cargo test --bin fuxx cli::", Passed{2}
    //   → RowWords { title: "Run the CLI tests", trace: Some("cargo test --bin fuxx cli::"), chips: vec!["2 passed"] }
    // Edit of link_route.rs, plus 41 minus 0, no said → title "Edited link_route.rs", trace None, chips ["+41 −0"]
    // build failure → chips ["build failed · E0425"]; running → title is running_title(); a lane row → None of this (drawn as today)
}
#[test]
fn a_column_s_tip_is_its_row_outcome_and_time() {
    // tests cli:: Passed{2}, took 9 s → "Ran tests cli:: · 2 passed · 0:09"; no time → "Ran tests cli:: · 2 passed"
}
#[test]
fn the_step_print_changes_when_a_call_its_clearing_or_the_width_changes() {
    // p = step_print(items, calls, &open, true); it differs when: a call finishes (fingerprint changes),
    // `open` loses one of the step's calls, or `fits` is false; it is equal for the same inputs.
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --bin fuxx -- agent_steps:: agent_chat::`
Expected: compile errors.

- [ ] **Step 3: Implement**

- `role_at`: a step is a maximal run of consecutive tool rows none of which `alone`; with `fits` and the run's length ≥ `TRACKS_FROM`, its first row is `Tracks(calls)` and the rest `Inside`; otherwise each is `Row(shape)`. Rows that stand alone are `Row(Shape::Solo)`; `group_shape_at` applies the same breaking (so mixed runs split at lane rows everywhere).
- `tracks_of`: one column per call; per touch (keyed by `file_key`) a bead on that file's track: Read→`Read`, Searched→`Search`, Changed→`Edit`, ScriptTouched→`Script`; a running call → `Running` on its tracks (and on Checks when it is a check); `failed` sets `Bead.failed`; `not_run` or stopped (not running, `ok == None`) sets `dim`. Check calls bead the Checks track: Passed→`Passed`, real Failed→`Failed`, planned→`Planned`, Unknown→`Check`. Files: changed first by their first successful change's column, then the rest by first touch. `counts`: actions = calls, files = distinct keys, failed = calls with `real_failure()`. `span` only when every call has a start time. `first_error`: the latest call of the step in `open`, its check's `first_error`.
- `file_summary` parts in order changed ("edited" + " at :N" + " · +P −M" sums of known counts; a delete "deleted", a move "moved"), read ("read" + up to two ranges joined ", ", "…" after), searched ("searched" + " · N matches" when known), script ("script N×" with other parts, else "touched by a script" / "touched by a script N×"); a file touched only by failed calls → "<verb> failed"; joined " · ".
- `checks_summary`: each run's `outcome_phrase`, consecutive equals collapsed, a planned failure followed by a pass of the same kind and scope folded into "red → green · {pass phrase}", joined " → ", at most the last 3 (a leading "… → " when cut).
- `outcome_phrase`: Passed{n} "{n} passed" / "passed"; Failed with `error_code` → "build failed · E0425" (in `checks_summary` the code follows a space: "build failed E0425"); Failed{failed, of} "{failed} of {of} failed"; else "{tests|build|lint} failed"; planned "red, as planned"; a failed non-check call "failed".
- `open_failures`: tool items from `from` with a real check failure, minus those followed (anywhere in the tab, from `from`) by a Passed check of the same `what` and `scope`.
- `row_words`: title = `running_title()` while running else `title()`; trace = the command when the title is the agent's own words or differs from it (never for Read/Edit/Search tools); chips = "+P −M" for a change with counts, "N matches", `outcome_phrase`, or the failed line's "failed".

- [ ] **Step 4: Run them and see them pass**

Run: `cargo test --bin fuxx -- agent_steps:: agent_chat::`
Expected: PASS (the existing `a_run_of_tools_is_one_group_and_steps_are_skipped` still passes).

- [ ] **Step 5: Run the gate**

- [ ] **Step 6: Commit**

```bash
git add src/agent_steps.rs src/agent_chat.rs src/agent_rows.rs
git commit -m "Chore: the step model — calls grouped into steps, file tracks with beads and summaries, a header, open failures and readable row words"
```

---

### Task 4: Drawing tracks and readable rows

**Files:**
- Create: `src/agent_tracks.rs` (a submodule of `agent_tab` like `agent_rows.rs`: `#[path = "agent_tracks.rs"] mod tracks;` — the group's drawing plus its pure geometry)
- Modify: `src/agent_tab.rs` (`Row.tracks: Option<Rc<Tracks>>` filled in `sync_rows` for dirty `Role::Tracks` rows; the row print adds `role` and `step_print`; `pane.fits`; new `Colors` fields; `tracks_focus: FocusHandle`, `track_cursor: Option<TrackCursor>`, `tracks_all: HashSet<String>` (steps opened to all columns), `files_all: HashSet<String>`; actions; the narrow-crossing re-sync in the `bar_width` canvas)
- Modify: `src/agent_rows.rs` (`item_row`: an `Inside` row draws an empty zero-height element; `tool()`: `Role::Tracks` → `self.tracks(…)`, `Role::Row` → today's row with `row_words`)
- Modify: `src/ui_tokens.rs` (`chat::` track tokens), `src/main.rs` (key bindings, the key-context stack test's list)

**Interfaces:**
- Consumes: Task 3's `Role`, `role_at`, `stands_alone`, `Tracks`, `tracks_of`, `open_failures`, `turn_start`, `step_print`, `row_words`, `header`, `Glyph`; Task 2's `tool_times`; existing `open_tool(id, cx)`, `tool_marks` (the running `MarkView`), `crate::sidebar_toggle::Tip`.
- Produces:

```rust
// src/ui_tokens.rs, mod chat
pub const TRACK_HEAD: f32 = 22.0;
pub const TRACK_ROW: f32 = 22.0;
pub const BEAD: f32 = 8.0;
pub const TRACK_LABEL_W: f32 = 112.0;
pub const TRACK_COL_W: f32 = 12.0;
pub const TRACKS_NARROW: f32 = 560.0;   // the chat column under this: rows

// src/agent_tracks.rs (pure, tested there)
pub fn fits_tracks(bar_width: f32) -> bool;                // bar_width.min(COLUMN_MAX) >= TRACKS_NARROW
pub fn column_x(col: usize, first_shown: usize) -> f32;    // TRACK_LABEL_W + (col - first_shown) * TRACK_COL_W
pub fn cursor_moved(col: Option<usize>, dir: i8, shown: std::ops::Range<usize>) -> usize; // None → the last shown
pub fn glyph_text(g: Glyph) -> &'static str;               // ● ▲ ■ ◆ ✓ ✕ ○ ● ("" for Running: the arc)
pub struct TrackCursor { pub lead: String, pub col: usize } // lead: the step's first call id
impl AgentTab { pub(super) fn tracks(&self, ix: usize, tracks: &Tracks, look: &Look) -> AnyElement; }

// src/agent_tab.rs
actions!(agent_tab, [ /* … */ TrackPrev, TrackNext, TrackOpen, TrackLeave ]);
pub bead: Rgb, pub bead_edit: Rgb, pub track_rule: Rgb, pub track_cursor: Rgb, // in Colors
#[cfg(feature = "ui-shots")] pub fn shot_tracks(&self) -> Vec<Rc<Tracks>>;  // the shown view's step models, in order
```

- [ ] **Step 1: Write the failing tests**

In `src/agent_tracks.rs`:

```rust
#[test]
fn tracks_fall_back_to_rows_under_560() {
    assert!(!fits_tracks(559.0));
    assert!(fits_tracks(560.0));
    assert!(fits_tracks(1400.0));
}
#[test]
fn columns_sit_after_the_label_column() {
    assert_eq!(column_x(36, 36), chat::TRACK_LABEL_W);
    assert_eq!(column_x(37, 36), chat::TRACK_LABEL_W + chat::TRACK_COL_W);
}
#[test]
fn the_cursor_moves_within_the_shown_columns() {
    assert_eq!(cursor_moved(None, 1, 36..60), 59);
    assert_eq!(cursor_moved(Some(36), -1, 36..60), 36);
    assert_eq!(cursor_moved(Some(59), 1, 36..60), 59);
    assert_eq!(cursor_moved(Some(40), 1, 36..60), 41);
}
#[test]
fn every_glyph_but_the_arc_has_its_shape() {
    assert_eq!([Glyph::Read, Glyph::Search, Glyph::Edit, Glyph::Script, Glyph::Passed, Glyph::Failed, Glyph::Planned].map(glyph_text),
        ["●", "▲", "■", "◆", "✓", "✕", "○"]);
    assert_eq!(glyph_text(Glyph::Running), "");
}
```

In `src/agent_tab.rs`'s `the_tab_s_text_meets_the_floor_in_every_theme`, add to the floors list:

```rust
("bead", c.bead, c.panel, MARK_MIN),
("bead hovered", c.bead, c.row_hover, MARK_MIN),
("bead edit", c.bead_edit, c.panel, MARK_MIN),
("bead edit hovered", c.bead_edit, c.row_hover, MARK_MIN),
("passed bead hovered", c.tool_ok, c.row_hover, MARK_MIN),
("track cursor", c.track_cursor, c.panel, MARK_MIN),
("track rule", c.track_rule, c.panel, QUIET_LINE_MIN),
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --bin fuxx agent_tab::`
Expected: compile errors.

- [ ] **Step 3: Implement**

- **The group** (same bordered group: `panel` fill, 1 pt `panel_border`, `R_GROUP`, `TOOL_PAD_X`): a `TRACK_HEAD` header row in `panel_dim` (`header(counts, took)` with `took` from `span` and `look.now` while running); a Checks track when any; the file tracks (`shown_files`), then a "+N files" row when N > 0; under the group's bottom edge, `first_error` in `failed` ink (one line, truncated, whole in its tooltip). Each track: the label (`panel_text`, truncated; the full path in a `Tip`), the beads at `column_x`, a `track_rule` line from the first to the last bead, the summary at the right (`panel_dim`, truncated, whole in a `Tip`).
- **Beads:** an 8 pt glyph centred in its column; ink: Edit → `bead_edit`, Passed → `tool_ok`, Failed / Planned and the small ✕ mark → `failed_panel`, the rest → `bead`; `dim` beads at the panel-dim ink; Running → the call's `tool_marks` `MarkView` at `BEAD` size (still under reduce motion, as `MarkView` is).
- **Columns:** each shown column is a full-height transparent strip (`id(("agent-track-col", ix))`): hover fills `row_hover` and shows `column.tip` in a `Tip`; a click opens `open_tool(column.id)` and sets `track_cursor`; the cursor column draws a 1 pt `track_cursor` ring. "+M earlier" is a cell left of the first shown column; a click toggles the step's id in `tracks_all` (the group then shows every column in a sideways `overflow_x_scroll`, same hover / click / ←/→). "+N files" toggles `files_all` likewise.
- **Keys:** the group's element has `key_context("AgentTracks")` and `track_focus(&self.tracks_focus)` while `track_cursor.lead` is its step; a click on its header or a column focuses it. main.rs binds `left` → `TrackPrev`, `right` → `TrackNext`, `enter` → `TrackOpen`, `escape` → `TrackLeave` in `Some("AgentTracks")`; add `"AgentTracks"` to the key-context stack test's list. `TrackLeave` clears the cursor and refocuses the box.
- **Rows (1–2 calls, and narrow):** today's 30 pt row with `row_words`: status mark, title in `tool_name` ink, the `trace` in a dim mono font filling the middle (truncated), the chips on the right (failure chips in `failed_panel`). A lane row and a "Passed on" row draw exactly as today.
- **Cache:** `sync_rows` computes `stands_alone` once per call (a `HashSet<&str>` of lane and passed-on ids), `open = open_failures(items, turn_start(items))`, each row's `role_at(…, fits_tracks(bar_width))`; a row's print adds `role` and, for `Tracks`, `step_print`; dirty `Tracks` rows rebuild `tracks_of(…, &self.tool_times, &self.start.cwd)` into `Row.tracks`. Drawing only reads `Row.tracks`. The `bar_width` canvas also calls for a re-sync (`cx.notify` through a flag the next render reads) when `fits_tracks` flips.

- [ ] **Step 4: Run them and see them pass**

Run: `cargo test --bin fuxx agent_tab::`
Expected: PASS.

- [ ] **Step 5: Run the gate**

- [ ] **Step 6: Commit**

```bash
git add src/agent_tracks.rs src/agent_tab.rs src/agent_rows.rs src/ui_tokens.rs src/main.rs
git commit -m "Chore: busy steps draw as file tracks — beads per call, summaries, hover and click per column, arrow keys, +earlier and +files — and small steps as readable rows"
```

---

### Task 5: The turn ledger in the folder tab, and its popover

**Files:**
- Create: `src/agent_ledger.rs` (the pure ledger model, parts and fitting)
- Modify: `src/agent_tab.rs` (`status_pill` appends the ledger parts; the ledger cached in `after_change` for the shown view; `AgentTabEvent::OpenDiff(PathBuf)`; `Colors::ledger_failing`; `#[cfg(feature = "ui-shots")] shot_ledger()`)
- Modify: `src/agent_box.rs` (`BoxMenu::Ledger`; a click on the pill opens it through `menu_frame`; `BoxTexts.ledger: Vec<String>`)
- Modify: `src/agent_box_menus.rs` (`ledger_menu(&self, cx) -> AnyElement` on `overlay_panel::shell`)
- Modify: `src/workspace_agent.rs` (`AgentTabEvent::OpenDiff(path)` → `open_file_tab(path, FileMode::Diff, …)`)

**Interfaces:**
- Consumes: Task 3's `file_key`, `turn_start`, `open_failures`, `outcome_phrase`, `ToolTimes`; Task 2's `Conversation::live_from`, `latest_you`; `crate::agent_lanes::{View, clock}`; existing `menu_frame`, `close_menu`, `heading`, `menu_row` in `agent_box_menus.rs`.
- Produces:

```rust
pub struct Running { pub id: String, pub title: String, pub started: Option<Instant>, pub more: usize }
pub struct Changed { pub path: PathBuf, pub plus: Option<u32>, pub minus: Option<u32> }
pub struct CheckRun { pub ix: usize, pub what: CheckWhat, pub scope: String, pub outcome: Outcome, pub planned: bool }
pub struct Ledger {
    pub running: Option<Running>,
    pub changed: Vec<Changed>,     // by successful calls, one per file_key, +/− summed over known counts
    pub scripted: Vec<PathBuf>,    // touched by scripts only (not changed)
    pub read: Vec<PathBuf>,        // read or searched only
    pub checks: Vec<CheckRun>,
    pub passed: usize,             // P: passed runs
    pub total: usize,              // T: all runs but planned failures and Unknown
    pub failing: Option<CheckWhat>,// the latest check of a kind failed for real and is still open
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind { Running, Changed, Checks }
pub struct Part { pub kind: PartKind, pub text: String }
pub const CHAR_W: f32 = 7.0;      // 12 pt medium, per character, for fitting
pub const PILL_EXTRA: f32 = 34.0; // a pill's icon, padding and gap
pub const SEP: &str = " · ";
pub fn ledger_of(conversation: &Conversation, view: &View, times: &ToolTimes, cwd: &Path) -> Option<Ledger>; // None: no live turn
pub fn parts(ledger: &Ledger, now: Instant) -> Vec<Part>;
pub fn fit(parts: &[Part], room: f32) -> usize;  // how many of `parts` show, cut from the right; Running always kept
pub fn room(column_w: f32, status_word: &str, folder: &str, branch: Option<&str>) -> f32;

// src/agent_tab.rs
AgentTabEvent::OpenDiff(PathBuf)
pub ledger_failing: Rgb, // in Colors
// src/agent_box.rs
BoxMenu::Ledger
```

- [ ] **Step 1: Write the failing tests** (in `src/agent_ledger.rs`; a `Conversation` fed events: `You`, tool starts / finishes with activities, lanes via `SubagentStarted`; `CWD = /tmp/fx/code/my-app`)

```rust
#[test]
fn main_s_ledger_covers_the_whole_tab_and_a_lane_only_itself() {
    // Main edits src/a.rs; lane L edits src/b.rs
    // ledger_of(Main).changed paths == [a, b]; ledger_of(Lane(L)).changed == [b]
}
#[test]
fn parallel_subagents_editing_one_file_count_it_once() {
    // lane A: Edit "/tmp/fx/code/my-app/src/api.rs" plus 2 minus 1; lane B: Bash "sed -i '3s/a/b/' src/api.rs"
    let main = ledger_of(&c, &View::Main, &times, cwd).unwrap();
    assert_eq!(main.changed.len(), 1);
    assert_eq!(main.changed[0].path, PathBuf::from("/tmp/fx/code/my-app/src/api.rs"));
    assert_eq!((main.changed[0].plus, main.changed[0].minus), (Some(2), Some(1)));
    assert_eq!(ledger_of(&c, &View::Lane("A".into()), &times, cwd).unwrap().changed.len(), 1);
    assert_eq!(ledger_of(&c, &View::Lane("B".into()), &times, cwd).unwrap().changed.len(), 1);
}
#[test]
fn a_failed_edit_is_not_counted_changed() {
    // a failed Edit of src/cli.rs → changed is empty, the file not in read either
}
#[test]
fn script_touched_files_are_listed_apart_and_never_counted() {
    // the read-only heredoc on src/api_client.rs → changed empty, scripted == [/tmp/fx/code/my-app/src/api_client.rs]
}
#[test]
fn checks_count_passed_over_runs_planned_and_unknown_apart() {
    // pass, real fail, planned fail, Unknown, pass → passed 2, total 3, checks.len() 5
}
#[test]
fn failing_holds_until_the_same_kind_and_scope_passes() {
    // tests "cargo test cli::" fail, lint pass → failing == Some(CheckWhat::Tests)
    // then tests "cargo test cli::" pass → failing == None
}
#[test]
fn the_latest_running_call_leads_with_plus_n_running() {
    // two running calls (lane A's started first, Main's later) → running.title == Main's running_title(), more == 1
    // lane rows themselves are never a running call
}
#[test]
fn no_ledger_for_a_restored_turn() {
    // past([You, tool]) → None; then you() + a tool → Some
}
#[test]
fn parts_read_running_changed_checks() {
    // running "Running tests" started 14 s ago with one more; 4 changed; passed 2 of 6
    assert_eq!(parts(&l, now).iter().map(|p| p.text.as_str()).collect::<Vec<_>>(),
        ["Running tests 0:14 · +1 running", "4 changed", "checks 2/6"]);
    // failing Tests → the last part reads "tests failing"; Build → "build failing"
    // nothing running, nothing changed, no checks → no parts
}
#[test]
fn it_trims_from_the_right_keeping_the_running_call() {
    // the three parts above (31, 9 and 10 characters); a fit counts the leading SEP and one SEP between parts:
    // 3 parts = (3 + 31 + 3 + 9 + 3 + 10) * CHAR_W = 413, 2 parts = 322, 1 part = 238
    assert_eq!(fit(&p, 413.0), 3);
    assert_eq!(fit(&p, 412.0), 2);
    assert_eq!(fit(&p, 322.0), 2);
    assert_eq!(fit(&p, 321.0), 1);
    assert_eq!(fit(&p, 0.0), 1);   // the running call is kept
    assert_eq!(fit(&p[1..], 0.0), 0);
}
#[test]
fn the_room_is_the_column_less_the_status_folder_and_branch() {
    assert_eq!(room(760.0, "Done", "my-app", Some("main")), 760.0 - (4.0 + 6.0 + 4.0) * CHAR_W - 3.0 * PILL_EXTRA);
}
```

In `agent_tab.rs`'s contrast test add `("ledger failing", c.ledger_failing, c.outer, TEXT_MIN)` and `("ledger failing on a pill under the pointer", c.ledger_failing, c.pill_hover, TEXT_MIN)`.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --bin fuxx -- agent_ledger:: agent_tab::`
Expected: compile errors.

- [ ] **Step 3: Implement**

- **Scope:** `ledger_of` is None unless `latest_you` exists and is ≥ `live_from`. Main: every tool item after `turn_start` (Main's and every lane's), lane and passed-on rows excluded. A lane: its own tool items after its latest `Item::Resumed` (else all of them). Paths are `file_key`ed.
- **Fitting:** `room = column_w − (chars(status_word) + chars(folder) + chars(branch)) × CHAR_W − (2 + branch.is_some()) × PILL_EXTRA` (the status pill, the folder pill, the branch pill); `fit` keeps the largest k whose `(SEP + parts[..k] joined by SEP)` chars × `CHAR_W` ≤ room, never fewer than the Running part when it is first.
- **Running:** the running tool with the latest start in `times` (else the last running in item order); `more` = the other running ones.
- **The pill:** `status_pill` keeps its mark and word first; when the ledger is Some and no question or permission is open, it appends `SEP` + the first `fit(parts, room(…))` parts in `c.dim` (a `failing` part in `ledger_failing`), the running part's clock drawn from `started` at the draw. The pill is clickable while the ledger shows (`BoxMenu::Ledger`); `BoxTexts.ledger` holds the drawn part texts. The ledger is rebuilt in `after_change` (events) and held on the tab; draws never rebuild it. It stays after the turn ends until the next send (the next `You` starts a new turn).
- **The popover:** `menu_frame(Some(ledger_menu), None, 320.0, cx)`, contents in sections with `heading`: "Changed" (path relative to `start.cwd`, then "+P −M" in `dim`; click → `close_menu` and `cx.emit(AgentTabEvent::OpenDiff(path))`), "Touched by scripts", "Read", "Checks" (scope, then `outcome_phrase`; real failures in `failed`), "Planned failures" (apart). Empty sections are left out. Esc closes it as every `BoxMenu` does.
- **Workspace:** `AgentTabEvent::OpenDiff(path)` → `self.open_file_tab(path, crate::layout_model::FileMode::Diff, window, cx)`.

- [ ] **Step 4: Run them and see them pass**

Run: `cargo test --bin fuxx -- agent_ledger:: agent_tab::`
Expected: PASS.

- [ ] **Step 5: Run the gate**

- [ ] **Step 6: Commit**

```bash
git add src/agent_ledger.rs src/agent_tab.rs src/agent_box.rs src/agent_box_menus.rs src/workspace_agent.rs src/main.rs
git commit -m "Chore: the turn ledger in the box's folder tab — the running call, changed files and checks after the status — with a popover that opens a file's diff"
```

---

### Task 6: Scenes, docs and the final gate

**Files:**
- Create: `src/ui_shots/agent_activity.rs` (a new area: `FILE`, `SOURCES`, `SCENES`)
- Modify: `src/ui_shots/mod.rs` (`mod agent_activity;`), `src/ui_shots/registry.rs` (`AREAS` row, the import, scene names in `ORDER` after the `agent-lanes-*` ones)
- Modify: `docs/04-Development/93 - Milestone 11ar - Subagents.md` (a bullet under "Chores after M11ar"), `docs/06-Progress-Log/60 - Progress Log.md` (an entry)
- Modify: `tests/ui-shots-ref/` (new references; changed ones only where the lane-row breaking or the row wording changed a picture)

**Interfaces:**
- Consumes: `super::agent::{decode, feed, scene_sized}`, `super::{Kind, Scene, Themes}`, `AgentTab::{shot_tracks, shot_ledger, shot_feed, send, show_view}`, `BoxTexts.ledger`, the `claude-activity.jsonl` fixture (Task 2), `part(cx, tab, n, name)`.
- Produces: the scenes below, all `themes: Themes::Both`, `serial: false`, `kind: Kind::State` unless a check reads pixels (`Kind::Visual`), `sources` = `src/agent_tracks.rs`, `src/agent_steps.rs`, `src/agent_activity*.rs`, `src/agent_rows.rs`, `src/agent_tab.rs`, `src/agent_chat.rs`, `src/providers/claude.rs`, the fixture (and `src/agent_ledger.rs`, `src/agent_box.rs`, `src/agent_box_menus.rs` for the ledger ones).

- [ ] **Step 1: Write the scenes, each with its assertions**

| Scene | What it feeds | It asserts |
|---|---|---|
| `agent-tracks-busy` | the fixture's 8-call step, turn done | one step model; `counts` starts "8 actions"; Checks track present; labels in the changed-first order; the group's width ≤ `COLUMN_MAX` |
| `agent-tracks-running` | the fixture cut after the 5th call's start | the running column's arc part exists; nothing else animates (reduce motion set: two frames' pixels equal) |
| `agent-tracks-failure` | a step ending in a real `cargo test` failure | a `Failed` bead on Checks; the first-error line part under the group says `error[E0425]…`; ledger last part "tests failing" |
| `agent-tracks-planned` | "red first" narration, failing then passing check | the bead `Planned`; Checks summary "red → green · 2 passed"; no first-error line |
| `agent-tracks-more-files` | a step touching 8 files | 5 file tracks and a "+3 files" row; a click on it shows 8 |
| `agent-tracks-earlier` | a step of 30 calls | a "+6 earlier" cell; a click shows 30 columns and the group scrolls sideways; ←/→ move the cursor |
| `agent-tracks-narrow` | the busy step at `(520.0, 640.0)` | no step model drawn as tracks; 8 readable rows |
| `agent-steps-rows` | a 1-call and a 2-call step | row titles: the description first ("Run the CLI tests"), the dim trace, "+41 −0" chip |
| `agent-ledger-running` | a running turn | `BoxTexts.ledger[0]` starts "Running tests"; "4 changed"; "checks 2/6" |
| `agent-ledger-done` | the same turn finished | the pill reads Done then "4 changed", "checks 6/6"; no running part |
| `agent-ledger-trimmed` | a narrow box | parts cut from the right; the running part kept |
| `agent-ledger-popover` | a click on the pill | the panel shows; "Changed" lists `src/api.rs +2 −1`; a click on it emits `OpenDiff`; Esc closes |

- [ ] **Step 2: Run the new area's scenes and see them pass**

Run: `scripts/ui-shots.sh --changed master --png`
Expected: every `agent-tracks-*`, `agent-steps-rows` and `agent-ledger-*` scene passes in both themes; look at their PNGs under `target/ui-shots/` (busy, running, failure, planned, narrow and the popover at least) before recording any reference.

- [ ] **Step 3: Write the docs**

- Note 93, under "Chores after M11ar", one bullet: the agent activity view (the user's words "I love C"; what shows: tracks for 3+ calls, readable rows, the ledger and its popover; the provider-neutral `Activity`; the tests' names and the scenes; "On screen: not yet seen in the real window" until the user confirms).
- Progress Log: a `### 2026-11-24 — Chore: the agent activity view` entry at the top of "Entries" in the log's usual shape, and the "Current status snapshot" line updated.

- [ ] **Step 4: Run the full screenshot run**

Run: `scripts/ui-shots.sh`
Expected: every scene passes. Scenes whose picture changed because a lane row now stands alone, or because a tool row's wording changed, are looked at (`--png`) and their references re-recorded with `scripts/ui-shots.sh --update-refs` only when the change is the intended one; list those scenes in the Progress Log entry. Then delete `target/ui-shots/` PNGs.

- [ ] **Step 5: Run the gate**

- [ ] **Step 6: Commit**

```bash
git add src/ui_shots/agent_activity.rs src/ui_shots/mod.rs src/ui_shots/registry.rs tests/ui-shots-ref "docs/04-Development/93 - Milestone 11ar - Subagents.md" "docs/06-Progress-Log/60 - Progress Log.md"
git commit -m "Chore: the agent activity view's scenes in both themes, and the docs"
```

Then the workflow's end: one whole-branch review on the most capable model against the spec and this plan, one fix wave, the full `scripts/ui-shots.sh` again, the user's look at the real window, and the merge.
