---
tags: [development, plan, milestone]
milestone: M11ak
status: built
updated: 2026-11-18
---

# M11ak Plan — Agent box

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[83 - Milestone 11ak - Agent Box]] |
| **Status** | 🟨 built on branch `m11ak-agent-box`, waiting for your on-screen check; not merged (planned 2026-11-17, from master `ded19b9`) |
| **Tasks done** | 15 / 15 |
| **Baselines** | Task 0, on `1995aae`: `cargo test --bin fuxx` 1,840 passed, 1 ignored; clippy default 14 (bin) / 27 (test), `ui-shots` 16 / 29; `cargo fmt --check` clean |
| **▶ Pick up here** | The final review of the whole branch, then the merge (after your on-screen check in note 83) |
| **Decisions used** | Yours: approach A (live where the agent can, restart where it can't), "split it — box first, drop mic and main agent", `/` and `@` "include both in the box milestone", "approved". The design's "AI's calls" sections. Plan-time calls below (AI's). |
| **Needs your eyes** | 👀 the on-screen check in note 83, after Task 14 |

**Goal:** The agent tab's message box becomes where you choose where the agent works (folder, branch), how it works
(model, effort, mode) and what it costs you (usage), and where you reach the agent's `/` commands and `@` mentions —
for Claude and agy, never sending anything without your send.

**Architecture:** Each provider (`src/providers/<agent>.rs`) answers, as pure functions tested on Task 0's recorded
fixtures: its models, efforts, modes, commands, usage, which quick $0 commands read them, and whether a change of
model, effort, mode or folder applies live or by a restart. A GPUI global, `src/agent_catalog.rs`, runs those quick
commands off the UI thread (with a deadline) and keeps their answers per agent and folder (usage for 60 s). The box
is drawn by `src/agent_box.rs` (an `impl AgentTab` block plus its pure helpers, as `workspace_agent.rs` is for
`Workspace`); the folder picker's lists live in `src/folder_picker.rs`. Changes go through one path,
`AgentTab::choose`, which writes a control request, restarts on the same conversation, or waits for the turn to end.

**Tech Stack:** Rust 2024, GPUI (pinned rev `1d2a4b3f`, unchanged; its macOS open panel already allows New Folder —
`setCanCreateDirectories(true)` in `gpui_macos/src/platform.rs`; clipboard images arrive as
`ClipboardEntry::Image(Image { format, bytes, .. })`; dropped files as `ExternalPaths`). **No new crates:** base64
and RFC 3339 are small pure functions with test vectors here.

**Spec:** [[83 - Milestone 11ak - Agent Box]]

## Plan-time calls (AI's, made while reading the code)

1. **The tab's agent already runs before your first message.** `AgentTab::booted` spawns the process at once (and
   Claude's `hello()` sends `initialize`). So §11's probe is needed only where the tab's own process can't answer:
   Claude's catalog comes from its own `initialize` response (`Event::Catalog`); the separate Claude probe runs only
   when the tab has no process (paused, held, failed). agy's lists always come from `agy models` and `/help`. "Before
   the first message nothing restarts" becomes: a change before the first message restarts the idle process (no
   conversation exists, nothing is lost) or, if live, is sent to it — as `next_mode` does today.
2. **Model and effort are saved with the tab.** `TabKind::Agent` / `SavedTab::Agent` gain `model` and `effort`
   (serde default, not written when none), so a resumed tab (and agy, whose flags are its only memory) starts with
   what you chose.
3. **fuxx passes `--model`, `--effort` and the mode flag only when you chose one**; until then the agent's own
   settings rule (Claude's mode chip shows the `current_permission_mode` its `initialize` reports). A preset's own
   `--model` / `--effort` / mode flag passes through until you choose in the box.
4. **Recent is stored in `SavedState`** (`recent_folders`, serde default, not written when empty): the debug build's
   own `sessions-dev.json` keeps it apart from the released app's.
5. **Claude's `/usage` reset times stay Claude's words** ("Resets Nov 17 at 8:09pm", the `(zone)` dropped): they
   name an IANA zone fuxx can't convert without a time-zone database. A `rate_limit_event`'s `resetsAt` (unix time)
   replaces them with "Resets in …" for that limit. agy's `reset_time` (RFC 3339) is always "Resets in …".
6. **Pasted images for an agent that takes paths (agy)** are written to `$TMPDIR/fuxx-images/<uuid>.<ext>` and sent
   as that path.
7. **Probes get 30 s** (`PROBE_DEADLINE`), then their process group is killed and the menu says it couldn't read.
8. **`/clear` is not intercepted:** it is sent as today (the tab follows the new id, M11aa). The commands fuxx
   answers itself are each provider's `own_command` list: `/usage`, `/model`, `/effort`.

## Global Constraints

- Branch `m11ak-agent-box`, main checkout (no worktree). Commit per task; merge to master after the whole-branch
  review (standing word); pushing is the user's call, after the private-name grep of the outgoing diff.
- **Gate per task:** `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features
  ui-shots && cargo fmt --check` (no new warnings over Task 0's baselines), then `scripts/ui-shots.sh --changed
  master`, and the full `scripts/ui-shots.sh` before the commit of any task that touches drawing. Reference
  fingerprints in `tests/ui-shots-ref` are stale on this Mac ("looks different" is reported, not a failure) — **never
  `--update-refs`**.
- **Provider-specific code stays in `src/providers/<agent>.rs`.** Generic code asks the provider (its models, efforts,
  modes, usage, commands, how a change applies); it never matches an agent's name, flag or wire field.
- **M11p:** nothing visible changes inside any `render`; no always-on timer. Probes and usage start from an open,
  a folder change or a menu opening — never from drawing.
- Usage is kept **60 s**. Images over **5 MB** (5 × 1024 × 1024 bytes) are refused with a note. Recent keeps **up
  to 8** folders. Claude's Move needs **Claude ≥ 2.1.223** (`claude --version`, read once per app run).
- **Never send a message without your send:** choosing a model, effort, mode, folder, attachment or `/` item never
  writes `Action::Send`. **Changes wait for the turn to end** (no mid-turn switch).
- **No made-up menus:** a list fuxx couldn't read says "Couldn't read the list" (the model menu: "Couldn't read the
  model list"); while reading, "Loading…"; usage while fetching, "Checking…".
- **fuxx never writes Claude's or agy's settings files.** `~/.claude.json` is only read (its `projects` keys), and a
  missing or unreadable file is skipped.
- **ui-shots scenes use made-up data only:** folders `~/code/my-app` and `~/code/scratch`, made-up models and usage,
  never `~/.claude.json` or a real folder. **No work-project names** anywhere (code, tests, fixtures, docs).
  Task 0 records in `~/code/scratch` with the recorder's scrub and trim on.
- Test the app only with `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug: its own `sessions-dev.json`).
- Commits stage files by path (`git add <paths>`), no AI attribution lines.
- Exact copy (`·` is U+00B7 with spaces): placeholder `Ask anything, @ to mention, / for actions`; restart notice
  `Switched to <model label> · <effort>` (no `· <effort>` when none), `Switched to <mode label>`, `Moved to <~/path>`;
  `View Usage ▸`; `N% left`; `Resets in 3d 16h` / `Resets in 2h 5m` / `Resets in 59m` / `Resets now`;
  `Close to limit`; `Limit reached · resets 14:00`; `Usage isn't available for <label>`; `(default list)`;
  `applies to a new conversation`; `Move this conversation`; `New conversation`; `Image over 5 MB: <name>`.

## Review Focus

1. **You pick a model, mode or folder while the agent is working:** nothing is written until `TurnDone`, then the
   change applies once; the chip shows the old value until then. Pinned in Task 4
   (`a_change_while_working_waits_for_the_turn`, scene `agent-box-change-waits`).
2. **A probe hangs or the agent is missing/logged out** (no `agy` on PATH, a hook that blocks): the probe is killed
   at its deadline, the menu says it couldn't read, and two tabs on one folder never start two probes. Pinned in
   Task 3 (`a_probe_past_its_deadline_is_killed_and_fails`, `one_probe_per_agent_and_folder`).
3. **A restart on the same conversation fails** (agy no longer knows the id; the new flags are refused): the tab's
   existing bad-resume path runs, nothing you typed is lost. Pinned in Task 4 (scene
   `agent-box-restart-keeps-queued-text`).
4. **A moved Claude conversation, then fuxx restarts:** its history is still found although its transcript may live
   under the old folder. Pinned in Task 6 (`history_finds_a_moved_conversation`), shaped by Task 0's finding.
5. **The agent's `/usage` output changes shape:** an unreadable line is shown as text, never a number; output that
   isn't JSON at all is "Couldn't read usage". Pinned in Task 2 (`an_unreadable_usage_line_is_kept_as_text`,
   `garbage_usage_is_an_error`).

---

### Task 0: Confirm on the wire (fixtures, findings, baselines)

**Files:**
- Modify: `scripts/record-agent-fixture.py` — new scenarios: claude `initialize` (send `initialize`, stop at its
  `control_response`), `set-model`, `set-effort`, `set-mode`, `image`, `move`; agy `restart-flags`, `move`, `image`;
  and `--scrub-file <in> <out>` (the same scrub for a plain command's output)
- Create in `src/providers/fixtures/`: `claude-initialize.jsonl`, `claude-set-model.jsonl`, `claude-set-effort.jsonl`,
  `claude-set-mode.jsonl`, `claude-image.jsonl`, `claude-move.jsonl`, `claude-usage.json`, `claude-version.txt`,
  `agy-models.txt`, `agy-help.json`, `agy-usage.json`, `agy-restart-flags.jsonl` (+ `.stderr`), `agy-move.jsonl`,
  `agy-image.jsonl` (a `.stderr` beside any recording that wrote one)
- Modify: note 83 (a "Task 0 findings" section), this plan's "At a glance"

- [x] **Step 1: Baselines.** On the clean branch run the gate; write the test count (and ignored), clippy warning
  counts (bin / test, default and `ui-shots`) and fmt state into "At a glance".
- [x] **Step 2: Record Claude** in `~/code/scratch` (made fresh; a sub-folder `~/code/scratch/moved` for `move`):
  - `initialize`: the response (expect `models[]` with `value`, `displayName`, `resolvedModel`, `supportedEffortLevels`;
    `commands[]`; `agents[]`; `current_permission_mode`), and how long it took.
  - `set-model`: before any message, `{"type":"control_request","request_id":"fuxx-set-1","request":{"subtype":"set_model","model":"sonnet"}}`;
    then "Say hi in one word"; then after `result`, `set_model` back to `opus` and one more one-word message. Record
    what answers each request (a `control_response` with that `request_id`? an error?) and the model `init`/`result`
    report afterwards.
  - `set-effort`: the documented effort change (try the SDK's flag-settings request carrying the effort; record the
    exact shape that is accepted, or that none is).
  - `set-mode`: `{"subtype":"set_permission_mode","mode":"plan"}`, then ask to write `hello.txt`; what the stream says.
  - `image`: one user message whose `content` is `[{"type":"image","source":{"type":"base64","media_type":"image/png","data":<a 1×1 red PNG>}},{"type":"text","text":"What colour is this pixel? One word."}]`.
  - `move`: a one-word conversation in `~/code/scratch`, then `--resume <id>` from `~/code/scratch/moved`; then
    list `~/.claude/projects/` entries for both folders: where the transcript is afterwards.
  - `claude -p "/usage" --output-format json` → `claude-usage.json`; list `~/.claude/projects/<scratch>/` before and
    after: does it leave a transcript? Check `claude --help` for a flag that stops session persistence; if one
    exists, record again with it and confirm nothing is left.
  - `claude --version` → `claude-version.txt`.
- [x] **Step 3: Record agy:** `agy models` → `agy-models.txt`; `agy -p "/help" --output-format json` →
  `agy-help.json`; `agy -p "/usage" --output-format json` → `agy-usage.json`; `restart-flags`: a one-word
  conversation, then the same `--conversation <id>` with `--mode plan`, `--model <another id from agy-models.txt>` and
  `--effort low`, asking "What was my first message?" (does it carry on; do the flags take — the model `init` or
  `result` names, a plan-mode refusal); whether `--model <id with effort>` alone or `--model` + `--effort` is the form
  that takes; `move`: the same conversation from `~/code/scratch/moved`; `image`: a message naming a PNG's absolute
  path ("What colour is the image at <path>? One word.").
- [x] **Step 4: Scrub check.** `rg -n "$USER|/Users/" src/providers/fixtures/` finds nothing new; no folder name but
  `scratch` / `moved` / `/tmp/fx` appears.
- [x] **Step 5: Write "Task 0 findings" into note 83**, ending with this table filled in (each cell from a recording):

  | Change | Claude | agy |
  |---|---|---|
  | Model | Live / Restart | Restart / NextConversation |
  | Effort | Live / Restart | Restart / NextConversation |
  | Mode | Live / Restart | Restart / NextConversation |
  | Folder (Move) | Restart (≥ 2.1.223) / none | Restart / NextConversation |

  plus: the exact request shapes and what confirms each live change; whether live requests work before the first
  message; agy's model flag form; Claude's transcript location after a move; the `/usage` transcript answer and the
  flag used; the probe's duration. A live change that didn't work as documented is **Restart** (design §0).
- [x] **Step 6: Commit** `M11ak Task 0: wire fixtures and findings` (`git add scripts/record-agent-fixture.py
  <the fixture files listed above> "docs/04-Development/83 - Milestone 11ak - Agent Box.md" "docs/04-Development/Plans/M11ak Plan - Agent Box.md"`).

### Task 1: The provider's catalog — models, efforts, modes, commands, flags

**Files:**
- Modify: `src/providers/mod.rs` (types and trait methods below; `ProviderInfo::modes` becomes `&'static [ModeChoice]`;
  `Start` gains `model`, `effort`), `src/providers/claude.rs`, `src/providers/agy.rs`, every `Start { .. }` and
  `ProviderInfo` construction (compiler-led; `workspace_agent.rs`, `agent_tab.rs`, `ui_shots/agent.rs`)
- Modify: `src/layout_model.rs` (`TabKind::Agent` and `SavedTab::Agent` gain `model: Option<String>`,
  `effort: Option<String>`, serde default + skip when none), every `TabKind::Agent { .. }` pattern (compiler-led),
  `src/workspace_agent.rs::agent_start` (fills them)

**Interfaces:**
- Produces (in `providers/mod.rs`):
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub struct ModeChoice { pub id: &'static str, pub label: &'static str, pub about: &'static str }
  #[derive(Clone, Debug, Default, PartialEq)]
  pub struct Catalog {
      pub models: Vec<ModelChoice>,
      pub fallback_models: bool,           // the provider's built-in list ("(default list)")
      pub commands: Vec<AgentCommand>,
      pub mode: Option<String>,            // the mode the agent says it runs in
  }
  #[derive(Clone, Debug, PartialEq)]
  pub struct ModelChoice { pub id: String, pub label: String, pub resolves_to: Option<String>, pub efforts: Vec<EffortChoice> }
  #[derive(Clone, Debug, PartialEq)]
  pub struct EffortChoice { pub level: String, pub model_id: Option<String> } // model_id: agy's per-effort id
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum CommandKind { Command, Skill, Agent }
  #[derive(Clone, Debug, PartialEq)]
  pub struct AgentCommand { pub name: String, pub about: String, pub kind: CommandKind, pub aliases: Vec<String> }
  #[derive(Clone, Debug)]
  pub struct Probe { pub argv: Vec<String>, pub stdin: Vec<String>, pub until: Option<fn(&str) -> bool> }
  pub struct Start { /* as today */ pub model: Option<String>, pub effort: Option<String> }
  pub enum Event { /* as today */ Catalog(Catalog), Skills(Vec<String>) }
  // Provider, with defaults (the test provider `Plain` overrides none):
  fn catalog_probes(&self) -> Vec<Probe> { Vec::new() }
  /// Its own process's start answers the catalog (`Event::Catalog`), so no probe runs while it runs.
  fn catalog_on_start(&self) -> bool { false }
  fn read_catalog(&self, _outputs: &[String]) -> Result<Catalog, String> { Err("no list".into()) }
  fn fallback_models(&self) -> Vec<ModelChoice> { Vec::new() }
  /// A model choice and effort level → the `(--model, --effort)` values to pass. Default: `(model.id, effort)`;
  /// agy: the effort's `model_id` (and `--effort` only if Task 0 found it needed).
  fn model_flags(&self, model: &ModelChoice, effort: Option<&str>) -> (String, Option<String>) { /* default */ }
  ```
- `Event::Catalog`: Claude's translator decodes the `control_response` to `fuxx-init`; `Event::Skills`: the names
  in Claude's `system/init` `skills` (the cache marks those commands `Skill`).

- [x] **Step 1: Write the failing tests:**
  - `claude.rs` `the_initialize_answer_is_the_catalog`: `read_catalog` over `claude-slash.jsonl` (its line 9) and over
    `claude-initialize.jsonl` → models' ids `["default","opus","fable"]` (slash) with labels `Default (recommended)`,
    `Opus 5.5`, `Fable 5.2`, `resolves_to == Some("claude-opus-5-5")` for the first two, efforts
    `["low","medium","high","xhigh","max"]` each with `model_id: None`; commands: the 3 `commands` as `Command` then
    the 3 `agents` as `Agent` (first `claude`); `mode == Some("default")`; `fallback_models == false`.
  - `claude.rs` `no_models_falls_back_to_the_aliases`: an `initialize` answer without `models` → ids
    `["opus","sonnet","haiku","fable"]`, `fallback_models == true`; a probe output with no answer line → `Err`.
  - `claude.rs` `the_tabs_own_initialize_answer_is_a_catalog_event` (decode of `claude-slash.jsonl` yields one
    `Event::Catalog` and, from its `init`, `Event::Skills` with `ai-behavior-trees-utility-ai` first);
    `claude_catalog_comes_with_its_start` (`catalog_on_start()` true; agy false).
  - `claude.rs` `claude_modes_are_its_four`: ids `default, acceptEdits, plan, auto`, labels `Ask before edits`,
    `Accept edits`, `Plan`, `Auto`; `default_mode == None`.
  - `claude.rs` `argv_passes_model_effort_and_mode_only_when_chosen`: none set → no `--model`/`--effort`/
    `--permission-mode` (a preset's `--model x` passes through); set → `--model sonnet --effort low
    --permission-mode plan`, and the preset's `--model x` is dropped.
  - `agy.rs` `agy_models_group_by_name_with_effort_ids`: `agy-models.txt` → the `claude-opus-5-5-high` line is an
    effort `high` with `model_id == Some("claude-opus-5-5-high")` of the model labelled `Claude Opus 5.5`; inline
    `"m-1\tPlain Model\n"` → a model `Plain Model` with no efforts; blank and malformed lines skipped.
  - `agy.rs` `agy_help_is_its_commands`: `agy-help.json` → every `commands[]` entry is a `Command` with its
    `aliases` and `description`; non-JSON → `Err`.
  - `agy.rs` `agy_argv_carries_the_chosen_model_and_mode` in the flag form Task 0 found (`--model <effort id>`, plus
    `--effort` only if Task 0 found it needed); `agy_modes_keep_their_names` (`accept-edits`, `auto`, `plan`).
  - `layout_model.rs` `an_agent_tab_keeps_its_model_and_effort` (round-trip; an old file without them loads).
- [x] **Step 2: Run** `cargo test --bin fuxx providers layout_model` — FAIL.
- [x] **Step 3: Implement** in the provider files; agy's `catalog_probes` are `["agy","models"]` and
  `["agy","-p","/help","--output-format","json"]` (outputs in that order); Claude's is its stream-json argv without
  resume, `stdin` the `initialize` line, `until` the line whose `request_id` is `fuxx-init`.
- [x] **Step 4: Run** — PASS; the gate.
- [x] **Step 5: Commit** `M11ak Task 1: providers say their models, efforts, modes and commands`.

### Task 2: Usage and rate limits (pure)

**Files:**
- Modify: `src/providers/mod.rs`, `src/providers/claude.rs`, `src/providers/agy.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq)] pub struct UsageGroup { pub name: String, pub about: Option<String>, pub limits: Vec<UsageLimit> }
  #[derive(Clone, Debug, PartialEq)]
  pub enum UsageLimit { Read { name: String, left_pct: u8, resets: Option<Reset> }, Text(String) }
  #[derive(Clone, Debug, PartialEq)] pub enum Reset { At(i64), Words(String) }   // unix seconds | the agent's words
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum LimitState { Fine, Close, Reached }
  #[derive(Clone, Debug, PartialEq)]
  pub struct RateLimit { pub limit: String, pub state: LimitState, pub resets_at: Option<i64> }
  pub enum Event { /* … */ RateLimit(RateLimit) }
  // Provider:
  fn usage_probe(&self) -> Option<Probe> { None }
  fn read_usage(&self, output: &str) -> Result<Vec<UsageGroup>, String>; // default Err
  // agy.rs (pure): pub fn rfc3339_unix(text: &str) -> Option<i64>;
  ```
- Claude's usage probe: `claude -p /usage --output-format json` plus Task 0's no-persistence flag if found. Its
  `RateLimit.limit` maps `five_hour` → `Current session`, `seven_day` → `Current week (all models)` (as Task 0's
  `/usage` text names them; any other type keeps its raw name); `allowed` → `Fine`, `allowed_warning` → `Close`,
  `rejected` → `Reached`.

- [x] **Step 1: Write the failing tests:**
  - `claude.rs` `usage_lines_read_as_percent_left`: inline result text `Current session: 0% used · resets Nov 17 at
    8:09pm (UTC)\nCurrent week (all models): 37% used · resets Nov 22 at 9:59pm (UTC)` → one group
    (name `""`) with `Read { "Current session", 100, Words("Nov 17 at 8:09pm") }` and `Read { "Current week (all
    models)", 63, Words("Nov 22 at 9:59pm") }`; `claude-usage.json` reads with at least one `Read`.
  - `claude.rs` `an_unreadable_usage_line_is_kept_as_text` (`Extra usage: off` → `Text("Extra usage: off")`; blank
    lines dropped); `garbage_usage_is_an_error` (`"not json"` → `Err`).
  - `claude.rs` `rate_limit_events_become_rate_limits`: `claude-slash.jsonl` lines 19 and 22 →
    `RateLimit { "Current week (all models)", Close, Some(1794754800) }` then `{ "Current session", Close,
    Some(1794535800) }`.
  - `agy.rs` `agy_usage_reads_groups_and_buckets`: `agy-usage.json` → groups named as its `name`s, each bucket
    `Read` with `left_pct == round(remaining_fraction × 100)` and `Reset::At(rfc3339_unix(reset_time))`; a bucket
    without `remaining_fraction` → `Text(name)`; non-JSON → `Err`.
  - `agy.rs` `rfc3339_reads_offsets_and_fractions`: `2026-11-22T14:59:00Z`, `2026-11-22T21:59:00+07:00` and
    `2026-11-22T14:59:00.123456+00:00` → `Some(1795359540)`; `1970-01-01T00:00:00Z` → `Some(0)`; `nonsense` → `None`.
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate.
- [x] **Step 5: Commit** `M11ak Task 2: providers read usage and rate limits`.

### Task 3: The start-up probe and the cache (`src/agent_catalog.rs`)

**Files:**
- Create: `src/agent_catalog.rs` (declared in `src/main.rs`; `cx.set_global(Catalogs::default())`)
- Modify: `src/agent_tab.rs` — the tab asks for its catalog after boot and on a folder change, observes the global
  (`cx.observe_global::<Catalogs>` → `cx.notify()`), feeds `Event::Catalog` / `Event::Skills` / `Event::RateLimit`
  from its own process into the global

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces:
  ```rust
  pub const USAGE_FRESH: Duration = Duration::from_secs(60);
  pub const PROBE_DEADLINE: Duration = Duration::from_secs(30);
  #[derive(Clone, Debug, PartialEq)] pub enum Lookup<T> { Loading, Ready(T), Failed(String) }
  #[derive(Default)] pub struct Catalogs { /* catalogs by (agent, folder); usage + when by agent; rate limits,
                                              version and known folders by agent */ }  // impl Global
  impl Catalogs {
      pub fn catalog(&self, agent: &str, folder: &Path) -> Option<&Lookup<Catalog>>;
      pub fn begin_catalog(&mut self, agent: &str, folder: &Path) -> bool;      // false: loading or ready already
      pub fn set_catalog(&mut self, agent: &str, folder: &Path, found: Lookup<Catalog>);
      pub fn mark_skills(&mut self, agent: &str, folder: &Path, skills: &[String]);
      pub fn usage(&self, agent: &str, now: Instant) -> Option<&Lookup<Vec<UsageGroup>>>; // None: absent or stale
      pub fn begin_usage(&mut self, agent: &str, now: Instant) -> bool;
      pub fn set_usage(&mut self, agent: &str, found: Lookup<Vec<UsageGroup>>, now: Instant);
      pub fn note_limit(&mut self, agent: &str, limit: RateLimit);              // replaces the same `limit`
      pub fn limits(&self, agent: &str) -> &[RateLimit];
      pub fn version(&self, agent: &str) -> Option<&Lookup<Option<String>>>;
      pub fn set_version(&mut self, agent: &str, found: Lookup<Option<String>>);
  }
  /// Run `probe` in `cwd` (PATH = `path_env`): its stdout to the `until` line (or the end). Err on a spawn failure,
  /// a non-zero exit with nothing read, or past `deadline` (its process group is killed).
  pub fn run_probe(probe: &Probe, cwd: &Path, path_env: Option<&str>, deadline: Duration) -> Result<String, String>;
  pub fn load_catalog(provider: &'static dyn Provider, folder: PathBuf, cx: &mut App);
  pub fn load_usage(provider: &'static dyn Provider, folder: PathBuf, cx: &mut App);
  pub fn load_version(provider: &'static dyn Provider, cx: &mut App);
  ```
  `load_*` call `begin_*`, then run on the background executor (login `PATH` from `workspace_ssh::login_env`, as the
  tab's boot), then `update_global`. `run_probe` uses `agent_process::spawn` and `kill()`.
  The tab's rule: with `catalog_on_start()` and its process starting, it marks the catalog loading (`begin_catalog`)
  and takes it from its own `Event::Catalog` (failed if the process ends without one); otherwise (agy always; Claude
  paused, held or failed) it calls `load_catalog`. Any `Event::Catalog` from a running agent refreshes the cache.

- [x] **Step 1: Write the failing tests** (`agent_catalog::tests`; probes are `/bin/sh -c …` stand-ins, never an agent):
  - `one_probe_per_agent_and_folder`: `begin_catalog` true once, false while loading and once ready; another folder
    or agent → true.
  - `usage_is_kept_sixty_seconds`: set at t → `usage(t + 59 s)` is `Some`, `usage(t + 60 s)` is `None` and
    `begin_usage` is true again.
  - `a_rate_limit_replaces_its_own_limit_only`.
  - `mark_skills_turns_those_commands_into_skills`.
  - `a_probe_reads_to_its_until_line` (`printf 'a\nb\nc\n'`, `until == "b"` → `"a\nb"`).
  - `a_probe_past_its_deadline_is_killed_and_fails` (`sleep 60`, deadline 300 ms → `Err` within 3 s; the pid is
    gone).
  - `a_missing_program_fails` (`fuxx-no-such-program` → `Err`).
- [x] **Step 2: Run** `cargo test --bin fuxx agent_catalog` — FAIL. **Step 3: Implement.**
- [x] **Step 4: Run** — PASS; the gate (agent-tab scenes unchanged: `for_shots` tabs never probe — they get a
  catalog through `shot_catalog`, Task 5).
- [x] **Step 5: Commit** `M11ak Task 3: the start-up probe and the per-agent, per-folder cache`.

### Task 4: Applying a change, live or by restart

**Files:**
- Modify: `src/providers/mod.rs` (below), `claude.rs` (encode the set requests in Task 0's shapes; decode their
  answers to `Event::Applied`; `applies` per Task 0's table), `agy.rs` (`applies` per the table; set actions
  encode nothing)
- Create: `src/agent_box.rs` (declared in `src/main.rs`; this task adds its pure change logic)
- Modify: `src/agent_tab.rs` — `choose`, a waiting change applied on `TurnDone`, restart via `spawn_process` with the
  updated `start`; `Action::Send` writes are untouched; under `ui-shots`, `write` records lines when `no_process`
- Modify: `src/workspace_agent.rs` — `AgentTabEvent::Settings` saves model, effort and mode into the tab's `TabKind`
  (as `Conversation` saves the mode today)
- Create: `src/ui_shots/agent_box.rs` (its `FILE`, `SCENES`; `mod agent_box;` in `src/ui_shots/mod.rs`; a row in
  `registry::AREAS`; every new scene name in `ORDER`, after the `agent-tab-*` ones — each later task adds its scenes
  here and to `ORDER`)

**Interfaces:**
- Produces:
  ```rust
  // providers/mod.rs
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum ChangeKind { Model, Effort, Mode, Folder }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Applies { Live, Restart, NextConversation }
  pub enum Action { /* as today */ SetModel { model: String }, SetEffort { effort: String }, SetMode { mode: String } }
  pub enum Event { /* … */ Applied { kind: ChangeKind, value: String, ok: bool } }
  fn applies(&self, change: ChangeKind) -> Applies { Applies::Restart }  // Provider
  // agent_box.rs
  #[derive(Clone, Debug, PartialEq)]
  pub enum Change { Model { id: String, effort: Option<String> }, Mode(String), Folder(PathBuf) }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Step { Live, RestartNow, AfterTurn, NextStart }
  pub fn step_for(applies: Applies, running: bool, spoken: bool, working: bool) -> Step;
  pub fn switched_notice(change: &Change, model_label: Option<&str>, mode_label: Option<&str>, home: Option<&Path>) -> String;
  // agent_tab.rs
  impl AgentTab {
      pub fn choose(&mut self, change: Change, cx: &mut Context<Self>);
      pub fn waiting_change(&self) -> Option<&Change>;
      #[cfg(feature = "ui-shots")] pub fn shot_written(&self) -> Vec<String>;
  }
  AgentTabEvent::Settings { model: Option<String>, effort: Option<String>, mode: Option<String> }
  ```
  `choose` turns a model change into `start.model` / `start.effort` with Task 1's `model_flags` (the catalog's
  `ModelChoice` by id). An effort-only change is `Change::Model` with the current model id and is asked as
  `ChangeKind::Effort`; a new model as `Model` (with a new effort too, the stricter answer of the two: `Live` only
  if both are). `step_for`: before the first message → `Live` if live and running, else `RestartNow`; working → `AfterTurn`; idle → `Live` if live and running, `RestartNow` if
  restart and running, else `NextStart`. `NextConversation` is always `NextStart` after the first message. A live
  change updates `start` only when `Applied { ok: true }` arrives; `ok: false` adds a notice `Couldn't switch:
  <why>` and changes nothing. A restart resumes `conversation_id()` and adds the switched notice; queued text is
  kept (as Force stop's restart). A folder change is only taken here when the folder is new.

- [x] **Step 1: Write the failing tests:**
  - `agent_box` `step_for_covers_every_case` (the rules above as a table: 3 `Applies` × running × spoken × working).
  - `agent_box` `switched_notices` (`Switched to Claude Sonnet 5.5 · medium`, `Switched to Claude Sonnet 5.5`,
    `Switched to Plan`, `Moved to ~/code/scratch`).
  - `claude.rs` `set_requests_encode_as_recorded` (each line equals Task 0's recorded request, ids `fuxx-set-<n>`
    rising); `set_answers_decode_to_applied` (from `claude-set-model.jsonl` / `-effort` / `-mode`: `Applied { ok:
    true }` with the value; an error answer → `ok: false`); `claude_applies_per_task_0`.
  - `agy.rs` `agy_set_actions_write_nothing`; `agy_applies_per_task_0`.
  - Scenes (in `src/ui_shots/agent_box.rs`): `agent-box-change-waits` (Claude, Working: choose a
    model → `shot_written()` gains nothing; feed `TurnDone` → the set request is written once);
    `agent-box-change-restart` (agy after a message, idle: choose `plan` → `shot_last_argv` has `--conversation <id>`
    and `--mode plan`, a `Switched to plan` row); `agent-box-restart-keeps-queued-text` (a message queued while the
    restart's process starts is sent after it).
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate.
- [x] **Step 5: Commit** `M11ak Task 4: changes apply live or by restart, after the turn`.

### Task 5: The box's layout

**Files:**
- Modify: `src/agent_box.rs` — `impl AgentTab { fn box_area(&self, c: &Colors, mono: SharedString, cx: &mut
  Context<Self>) -> AnyElement }` replacing `composer_area` (§1: top row folder ▾ and branch ▾ chips, shown as text
  until Tasks 6–7; the box with attachment chips on top, the text and the placeholder; bottom row `+`, model chip,
  then mode chip and send ➤ / today's Stop labels on the right); menus anchored to their chip, opening upward
  (`deferred(anchored())`), in the tab menu's look
- Modify: `src/agent_tab.rs` — `top_line` gains the cost and context (`$0.21 · context 18%`); the chips above the box
  go; the `/ commands` `@ files` hint row goes (the placeholder says it); `update_placeholder`'s idle text is the new
  placeholder; `AgentTab` gains `menu: Option<BoxMenu>`
- Modify: `src/composer.rs` — `Composer::set_bare(&mut self, bare: bool, cx)` (no border or fill: the box draws them)
- Modify: `src/ui_shots/agent_box.rs`, `src/ui_shots/registry.rs` (`ORDER`); `src/ui_shots/agent.rs` — scenes that
  read the old chips or hint row follow the new box

**Interfaces:**
- Produces:
  ```rust
  pub enum BoxMenu { Folder, Model { efforts_of: Option<usize>, usage: bool }, Mode, Attach, Move(PathBuf) }
  impl AgentTab {
      pub fn open_menu(&mut self, menu: BoxMenu, cx: &mut Context<Self>);   // one open at a time; Esc closes
      #[cfg(feature = "ui-shots")] pub fn shot_catalog(&mut self, found: Lookup<Catalog>, cx: &mut Context<Self>);
  }
  pub const PLACEHOLDER: &str = "Ask anything, @ to mention, / for actions";
  /// The catalog model (its place) and effort level that `start`'s flags name: a model whose id, or one of whose
  /// efforts' `model_id`, is `model_flag` (agy's ids carry the effort).
  pub fn choice_of(catalog: &Catalog, model_flag: &str, effort_flag: Option<&str>) -> Option<(usize, Option<String>)>;
  pub fn chip_model_label(catalog: Option<&Catalog>, start: &Start, reported: Option<&str>, provider: &dyn Provider) -> String;
  pub fn effort_word(level: &str) -> String;   // "high" → "High", "xhigh" → "XHigh"
  ```
  `chip_model_label`: the chosen model's label (`choice_of`); else the catalog model whose `resolves_to` (or id) is
  the reported model, preferring a non-default entry; else `provider.model_label(reported)`; else `Model`.

- [x] **Step 1: Write the failing tests:** `chip_model_label_prefers_the_named_model` (default resolving to
  `claude-opus-5-5` and an `opus` entry → `Opus 5.5`); `choice_of_finds_agy_effort_ids` (`claude-opus-5-5-high` →
  that model, `Some("high")`); `effort_words`.
- [x] **Step 2: Write the failing scenes** (dark and light, `Kind::Visual`, made-up catalog `Claude Example 5`):
  `agent-box-empty` (placeholder, `+`, model chip `Claude Example 5  High ▾`, mode chip, ➤),
  `agent-box-working` (■ Stop in send's place; top line `Working … $0.21 · context 18%`),
  `agent-box-attachment` (a command-block chip inside the box, its × removes it),
  `agent-box-long-text` (the box grows to 8 rows then scrolls; chips stay put).
- [x] **Step 3: Run** `scripts/ui-shots.sh --changed master` — the new scenes FAIL. **Step 4: Implement.**
- [x] **Step 5: Run** — PASS; unit tests; the gate; the full `scripts/ui-shots.sh`.
- [x] **Step 6: Commit** `M11ak Task 5: the agent box's layout`.

### Task 6: Folder chip and picker

**Files:**
- Create: `src/folder_picker.rs` (pure lists; `KnownFolders` global)
- Modify: `src/restore.rs` (`SavedState::recent_folders: Vec<PathBuf>`), `src/workspace.rs` (keeps `RecentFolders`, pushes on
  an agent tab's start and folder change and on a workspace opened on a folder; sets `KnownFolders` when they change;
  `save()` writes them)
- Modify: `src/workspace_agent.rs` — `new_agent_tab` / `agent_beside` start in the pane's terminal's folder now
  (`folder_now`), else `active_folder_now()`, else the project, else home; `AgentTabEvent::Folder(PathBuf)` (saved as
  the tab's `cwd`) and `AgentTabEvent::NewConversationIn(PathBuf)` (a new agent tab of the same provider in that
  folder, beside it in the pane)
- Modify: `src/agent_box.rs` (the chip `📁 <name> ▾`, the picker: Search on top (a `NameInput`), Recent, This
  Computer ▸, New Project (`prompt_for_paths(folder_panel())`), No Project (home), ✓ on the current; after the first
  message the `Move this conversation` / `New conversation` choice), `src/providers/claude.rs` (`known_folders`,
  `version_probe`, `can_move`; `history` also finds a moved transcript per Task 0), `src/agent_catalog.rs`
  (`load_known_folders`)

**Interfaces:**
- Produces:
  ```rust
  pub const RECENT_MAX: usize = 8;
  #[derive(Clone, Debug, Default, PartialEq)] pub struct RecentFolders { /* newest first */ }
  impl RecentFolders { pub fn push(&mut self, folder: PathBuf); pub fn folders(&self) -> &[PathBuf]; pub fn from_saved(v: Vec<PathBuf>) -> RecentFolders; }
  #[derive(Clone, Debug, Default)] pub struct KnownFolders { pub recent: Vec<PathBuf>, pub projects: Vec<PathBuf> } // impl Global
  #[derive(Clone, Debug, PartialEq)]
  pub enum PickerRow { Heading(&'static str), Folder { path: PathBuf, shown: String, current: bool }, ThisComputer, NewProject, NoProject }
  pub fn picker_rows(query: &str, recent: &[PathBuf], computer: &[PathBuf], current: &Path, home: Option<&Path>, exists: impl Fn(&Path) -> bool) -> Vec<PickerRow>;
  pub fn computer_folders(projects: &[PathBuf], agent_known: &[PathBuf]) -> Vec<PathBuf>; // deduped, sorted by shown path
  // Provider:
  fn version_probe(&self) -> Option<Probe> { None }
  fn can_move(&self, version: Option<&str>) -> bool { false }
  fn known_folders(&self, home: &Path) -> Vec<PathBuf> { Vec::new() } // reads files: off the UI thread
  // claude.rs (pure): pub fn version_at_least(text: &str, min: (u32, u32, u32)) -> bool;
  //                   pub fn claude_json_projects(text: &str) -> Vec<PathBuf>;
  // agent_catalog.rs:
  pub fn load_known_folders(provider: &'static dyn Provider, cx: &mut App);
  impl Catalogs { pub fn known_folders(&self, agent: &str) -> Option<&Lookup<Vec<PathBuf>>>;
                  pub fn set_known_folders(&mut self, agent: &str, found: Lookup<Vec<PathBuf>>); }
  #[cfg(feature = "ui-shots")] impl AgentTab { pub fn shot_place(&mut self, folder: PathBuf, branch: Option<String>, cx: &mut Context<Self>); }
  ```
  Opening the picker calls `load_known_folders` and `load_version` (each once per app run). `projects` in
  `KnownFolders` are the workspaces' and saved sessions' folders; agy adds none unless Task 0 found a list.
  Move is offered when `applies(Folder) == Restart` and `can_move(version)`; otherwise only New conversation. The
  picker's choice of the current folder does nothing.

- [x] **Step 1: Write the failing tests:**
  - `folder_picker` `recent_keeps_eight_newest_first_without_twins`; `recent_skips_missing_folders` (via `exists`);
    `search_filters_every_section` (fuzzy on the shown path; empty sections leave their heading out);
    `rows_mark_the_current_folder`; `computer_folders_dedupe`.
  - `claude.rs` `move_needs_2_1_223` (`2.1.222 (Claude Code)` → false, `2.1.223 (Claude Code)` and
    `claude-version.txt` → true, garbage → false); `claude_json_projects_are_its_project_keys` (inline JSON with
    two `projects` keys → both; garbage → empty); `history_finds_a_moved_conversation` (temp home: transcript only
    under the old folder's mangled dir, `start.cwd` the new one → history read) — only if Task 0 found the transcript
    stays under the old folder; otherwise the test asserts it is under the new one.
  - `restore.rs` `recent_folders_round_trip` (an old file without them loads).
  - Scenes (made-up `~/code/my-app`, `~/code/scratch`): `agent-box-folder-picker`, `agent-box-folder-search`
    (`scr` leaves `~/code/scratch`), `agent-box-folder-move-ask` (after a message: both choices),
    `agent-box-folder-move-new-only` (Claude `2.1.200`: only `New conversation`), `agent-box-folder-before-first`
    (choosing `~/code/scratch` restarts the idle process there: `shot_last_argv` cwd, no `--resume`).
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [x] **Step 5: Commit** `M11ak Task 6: folder chip and picker, Recent, Move or New conversation`.

### Task 7: Branch chip

**Files:**
- Modify: `src/agent_box.rs` (chip `⑂ <branch> ▾`, only with a branch), `src/agent_tab.rs` (`branch:
  Option<String>`, looked up with `git_branch::find_git` + `branch_at` on the background executor at boot, on a
  folder change and on `branch_changed`), `src/branch_switcher.rs` (`open_branch_switcher_in(folder: PathBuf, …)`,
  which `open_branch_switcher` calls; `refresh_branches_in` also tells agent tabs in that repo)

**Interfaces:**
- Consumes: Task 6's `shot_place` (scenes set the folder and branch with it).
- Produces: `AgentTabEvent::OpenBranchSwitcher(PathBuf)`; `AgentTab::branch_changed(&mut self, cx)`;
  `Workspace::open_branch_switcher_in(&mut self, folder: PathBuf, window: &mut Window, cx: &mut Context<Self>)`.

- [x] **Step 1: Write the failing scenes:** `agent-box-branch` (`⑂ master ▾` beside `📁 my-app ▾`; no branch → no
  chip); `agent-box-branch-opens-switcher` (clicking it opens the Workspace's branch switcher on that folder; the
  agent's process is not restarted — `shot_last_argv` unchanged).
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [x] **Step 5: Commit** `M11ak Task 7: branch chip opens the branch switcher`.

### Task 8: Model menu with effort

**Files:**
- Modify: `src/agent_box.rs` (the menu: `Model` heading, rows with name, current effort in grey, ✓, ▸ to the effort
  list of that model's levels only; a divider; `View Usage ▸` (opens nothing until Task 9); `Loading…` /
  `Couldn't read the model list` / the fallback list marked `(default list)`; with `NextConversation`, `applies to a
  new conversation` and a `New conversation` row); choosing calls `choose(Change::Model { .. })`

**Interfaces:**
- Consumes: Task 1's `Catalog`, `model_flags`; Task 4's `choose`; Task 5's `BoxMenu::Model`.
- Produces: `pub fn model_rows(catalog: &Lookup<Catalog>, start: &Start, applies: Applies) -> Vec<ModelRow>` (current
  by Task 5's `choice_of`)
  with `pub enum ModelRow { Heading, Model { ix: usize, label: String, effort: Option<String>, current: bool, has_efforts: bool }, Note(&'static str), NewConversation, Divider, Usage }`.

- [x] **Step 1: Write the failing tests:** `model_rows_loading_failed_and_fallback`; `model_rows_mark_the_chosen`.
- [x] **Step 2: Write the failing scenes:** `agent-box-model-menu` (made-up Claude list, ✓ and grey effort),
  `agent-box-effort-list` (▸ opens `Low … Max`), `agent-box-model-agy` (agy's grouped models),
  `agent-box-model-loading`, `agent-box-model-failed`, `agent-box-model-fallback`, `agent-box-model-choose-sends-nothing`
  (choosing writes no `Send` line; idle Claude writes the set request).
- [x] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [x] **Step 6: Commit** `M11ak Task 8: model menu with effort`.

### Task 9: View Usage

**Files:**
- Modify: `src/agent_box.rs` (the submenu: per group its name, each limit `N% left` with a small ring and
  `Resets …`, a `Text` limit as plain text; `Checking…` while `load_usage` runs; `Couldn't read usage`; `Usage isn't
  available for <label>` when `usage_probe()` is None; a rate limit's `Close to limit` / `Limit reached · resets
  HH:MM` under its row; a warning dot on the model chip while any limit is `Close` or `Reached`)

**Interfaces:**
- Consumes: Task 2's types, Task 3's `load_usage`, `Catalogs::usage`, `Catalogs::limits`.
- Produces: `pub fn resets_in(secs: i64) -> String`; `pub fn reset_text(reset: &Reset, now_unix: i64) -> String`
  (`Words(w)` → `Resets <w>`); `pub fn limit_line(limit: &RateLimit, utc_offset: i64) -> Option<String>`
  (`activity::clock_hm`; `Fine` → None); under `ui-shots`, `AgentTab::shot_usage(&mut self, found:
  Lookup<Vec<UsageGroup>>, limits: Vec<RateLimit>, cx)`.

- [x] **Step 1: Write the failing tests:** `resets_in_words` (316800 → `Resets in 3d 16h`, 7500 → `Resets in 2h 5m`,
  3540 → `Resets in 59m`, 0 and −5 → `Resets now`); `limit_lines` (`Close` → `Close to limit`; `Reached` at
  `14:00` local → `Limit reached · resets 14:00`).
- [x] **Step 2: Write the failing scenes:** `agent-box-usage-claude` (made-up 100 % / 63 % left, Claude's words),
  `agent-box-usage-agy` (two groups, rings), `agent-box-usage-checking`, `agent-box-usage-text-line`,
  `agent-box-usage-limit-close` (warning dot on the chip, `Close to limit` line), `agent-box-usage-cached` (opening
  again within 60 s starts no fetch).
- [x] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [x] **Step 6: Commit** `M11ak Task 9: View Usage`.

### Task 10: Mode chip

**Files:**
- Modify: `src/agent_box.rs` (chip `🛡 <label> ▾` — the chosen mode, else the catalog's `mode`, else the provider's
  default; the menu lists `info().modes` with their `about` lines and ✓), `src/agent_tab.rs` (`next_mode` and its
  "read-only after the first message" lock go; `shot_next_mode` becomes `shot_choose_mode`),
  `src/ui_shots/agent.rs` (`agent-tab-agy` uses it); with `applies(Mode) == NextConversation` the menu says
  `applies to a new conversation` and offers `New conversation`

**Interfaces:**
- Consumes: Task 4's `choose(Change::Mode(id))`; `ProviderInfo::modes`; `Catalog::mode`.
- Produces: under `ui-shots`, `AgentTab::shot_choose_mode(&mut self, id: &str, cx: &mut Context<Self>)`.

- [x] **Step 1: Write the failing scenes:** `agent-box-mode-menu` (Claude's four with descriptions),
  `agent-box-mode-agy-after-message` (choosing `plan` restarts on the same conversation per Task 4; no lock),
  `agent-box-mode-reported` (Claude with no choice shows the catalog's `default` as `Ask before edits`).
- [x] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [x] **Step 5: Commit** `M11ak Task 10: mode chip and menu`.

### Task 11: `+` attach

**Files:**
- Modify: `src/providers/mod.rs` (`Action::Send { text, images: Vec<Picture> }`; `fn sends_images(&self) -> bool
  { false }`), `src/providers/claude.rs` (`true`; a send with images is a `content` array of base64 image blocks then
  the text block, as Task 0's `claude-image.jsonl`), `src/providers/agy.rs` (unchanged: images never reach it)
- Modify: `src/agent_tab.rs` — `attachments: Vec<Attachment>` (today's command blocks become `Attachment::Text`);
  `attach` takes an `Attachment`; `wire_text` and `say` follow; paste (`ClipboardEntry::Image`) and drop
  (`ExternalPaths`) onto the box add chips; `src/workspace_agent.rs::attach_block` wraps its text
- Modify: `src/agent_box.rs` (the `+` menu: `File…` (`prompt_for_paths` files, multiple) and `Image…`; chips
  `📎 name` / `🖼 name`; the refusal note)

**Interfaces:**
- Produces:
  ```rust
  // providers/mod.rs
  #[derive(Clone, Debug, PartialEq)] pub struct Picture { pub media_type: &'static str, pub bytes: Arc<[u8]> }
  // agent_tab.rs
  pub const IMAGE_MAX: usize = 5 * 1024 * 1024;
  #[derive(Clone, Debug, PartialEq)]
  pub enum Attachment { Text(String), File(PathBuf), Image { name: String, picture: Picture, path: Option<PathBuf> } }
  impl Attachment { pub fn label(&self) -> String; }
  pub fn media_type_for(path: &Path) -> Option<&'static str>;   // png, jpeg/jpg, gif, webp
  pub fn wire_text(text: &str, attachments: &[Attachment], images_inline: bool) -> String;
  // claude.rs (pure): pub fn base64(bytes: &[u8]) -> String;
  ```
  `queued` and `Run::sends` carry `Vec<Attachment>`; the conversation's `you` gets their `label()`s. Without
  `sends_images`, an image goes as its path (a pasted one written to `$TMPDIR/fuxx-images/<uuid>.<ext>`
  first, off the UI thread). Over `IMAGE_MAX` → no chip, a notice `Image over 5 MB: <name>`.

- [x] **Step 1: Write the failing tests:** `claude.rs` `base64_vectors` (`f` → `Zg==`, `fo` → `Zm8=`, `foo` → `Zm9v`,
  `foobar` → `Zm9vYmFy`); `a_send_with_an_image_is_a_content_array` (same shape as `claude-image.jsonl`'s user line);
  `a_send_without_images_is_as_before`; `agent_tab` `wire_text_puts_paths_after_the_text`,
  `media_types_by_extension`, `an_image_over_5_mb_is_refused` (exactly `IMAGE_MAX` accepted, +1 refused).
- [x] **Step 2: Write the failing scenes:** `agent-box-attach-menu`, `agent-box-image-chip`, `agent-box-file-chip`,
  `agent-box-image-too-big` (the notice; no chip); and `agent-tab-attachment` follows the new chip.
- [x] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [x] **Step 6: Commit** `M11ak Task 11: attach files and images`.

### Task 12: `/` menu

**Files:**
- Modify: `src/providers/mod.rs` (`#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum BoxCommand { Usage, Model,
  Effort }`; `fn own_command(&self, typed: &str) -> Option<BoxCommand> { None }`), `claude.rs` / `agy.rs` (`/usage`,
  `/model`, `/effort`)
- Modify: `src/agent_box.rs` (the list: Commands, Skills, Agents headings, name and one-line description, the
  selected row; `Loading…` / `Couldn't read the list`), `src/agent_tab.rs` (open/filter/close from
  `composer_changed`; Return picks; Esc closes; picking inserts `/name `, or for an own command opens its menu and
  clears the box; a sent own command opens its menu instead of sending), `src/composer.rs` (`Complete` action →
  `ComposerEvent::Complete`), `src/main.rs` (`tab` → `composer::Complete` in `AtPopup > Composer`; the `/` list uses
  the `AtPopup` context and `AtUp` / `AtDown`); the list is `AgentTab::slash: Option<SlashList>` (as `at`), not a
  `BoxMenu`

**Interfaces:**
- Produces: `pub fn slash_query(text: &str, cursor: usize) -> Option<&str>` (text starts with `/`, caret inside the
  first word); `pub fn slash_rows(commands: &[AgentCommand], query: &str, limit: usize) -> Vec<SlashRow>` with
  `pub enum SlashRow { Heading(CommandKind), Item { ix: usize, positions: Vec<usize> } }` (fuzzy on name and aliases,
  `SLASH_LIMIT = 8` per group).

- [x] **Step 1: Write the failing tests:** `slash_query_only_at_the_start` (`/mo` caret 3 → `mo`; `a /mo` → None;
  `/model x` caret 9 → None); `slash_rows_group_and_filter`; `own_commands` (Claude `/usage` → `Usage`, `/clear` →
  None).
- [x] **Step 2: Write the failing scenes:** `agent-box-slash` (grouped, made-up commands), `agent-box-slash-filtered`,
  `agent-box-slash-tab-completes`, `agent-box-slash-usage-opens-menu` (no `Send` written).
- [x] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [x] **Step 6: Commit** `M11ak Task 12: the / menu`.

### Task 13: `@` menu with sections

**Files:**
- Modify: `src/agent_tab.rs` (`AtList` gains the catalog's agents and skills matching the query, fuzzy, up to 5 each;
  `pick_at` inserts `@name ` for those), `src/agent_box.rs` (headings Files, Agents, Skills; an empty section is left
  out), `src/ui_shots/agent.rs` (`agent-tab-at-files` follows the headings)

**Interfaces:**
- Produces: `pub fn at_rows(files: &[FileHit], commands: &[AgentCommand], query: &str) -> Vec<AtRow>` with
  `pub enum AtRow { Heading(&'static str), File(usize), Named { ix: usize, positions: Vec<usize> } }`.

- [x] **Step 1: Write the failing tests:** `at_rows_have_sections_in_order`; `an_agent_pick_inserts_at_name`.
- [x] **Step 2: Write the failing scenes:** `agent-box-at-sections` (made-up files, agents, skills).
- [x] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [x] **Step 6: Commit** `M11ak Task 13: the @ menu's agents and skills`.

### Task 14: Docs, verification, hand-off

**Files:**
- Modify: note 83 ("What was built", "On-screen check", status), `docs/04-Development/40 - Roadmap.md`,
  `docs/06-Progress-Log/60 - Progress Log.md`, this plan's table and status, module docs of `agent_box.rs`,
  `agent_catalog.rs`, `folder_picker.rs`, and the repo's module list if it lists modules

- [ ] **Step 1: Idle CPU (M11p):** an agent tab shown, idle, no menu open: `top -l 6 -s 5 -pid <fuxx>` median on the
  branch vs master, both in note 83; they match within noise.
- [x] **Step 2: No stray processes:** after opening five agent tabs and every menu, `pgrep -fl 'claude -p|agy'`
  shows only the tabs' own processes (probes ended).
- [x] **Step 3: Write note 83's on-screen check** (about 12 items): chips and placeholder; folder picker sections,
  New Project's New Folder, No Project; Move and New conversation; branch chip opens the switcher; model menu and
  effort; View Usage for Claude and agy; a model change mid-turn waits; agy mode after a message restarts on the same
  conversation; `+` file and image (and over 5 MB); paste and drop; `/` and `@`.
- [x] **Step 4: Full gate** and full `scripts/ui-shots.sh`; numbers into note 83 and the Progress Log; private-name
  grep of `git diff master...HEAD`.
- [x] **Step 5: Commit** `M11ak Task 14: docs and hand-off`.
