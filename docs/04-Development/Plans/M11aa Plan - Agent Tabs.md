---
tags: [development, plan, milestone]
milestone: M11aa
status: planned
updated: 2026-11-13
---

# M11aa Plan — Agent tabs and providers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[Proposal - Chat View and Agent Tabs]] — Part 1 only (Part 2 is M11ab, not planned) |
| **Status** | 📝 Planned 2026-11-13. **Not in [[12 - Scope]] / [[40 - Roadmap]] yet** — Task 0 puts it there. |
| **Order** | After M11z and after M11p (the proposal's "Where it goes"). |
| **Tasks done** | 0 / 14 |
| **▶ Pick up here** | Task 0, once the agent working on `master` is between tasks. |
| **Decisions used** | Yours (2026-11-13): chat view for agents (C → C1), fuxx launches the agent, Claude + agy first in their own provider files, one kind per tab, menu presets open a new workspace with a shell + the agent side by side, classic = type the command in a shell, M11aa in M11. AI's calls: the proposal's numbered list, plus "Plan-time calls" below. |
| **Needs your eyes** | 👀 the on-screen checklist written in Task 13 |

**Goal:** Picking **claude** or **agy** opens a workspace with a shell and an **agent tab** side by side; the agent
tab runs the agent in its structured mode and shows the conversation as a chat (streamed text, tool rows that open
to diffs/output, a todo checklist, an Allow / Always / Deny card), with the tab's status driving rings,
notifications, Activity and the scripting API.

**Architecture:** Pure cores first, GPUI last. `providers/` translates each agent's JSON lines into one fuxx
`Event` language and fuxx `Action`s back (Tasks 2, 12). `agent_conversation.rs` folds events into chat items and a
`Status`; `agent_markdown.rs` parses message text (Task 3). `agent_process.rs` runs the agent as a piped child with
a reader thread (Task 4). `TabKind::Agent` / `SavedTab::Agent` add the tab to the layout model and restore (Task 5).
`composer.rs` is a multi-line input (Task 6); `agent_tab.rs` is the view (Task 7); `workspace_agent.rs` wires it
into the workspace, menus, status, API and drag-and-drop (Tasks 8–11).

**Tech Stack:** Rust; GPUI at the pinned rev `1d2a4b3f…` — only APIs already used in this repo plus `gpui::list` /
`ListState` for the conversation (Task 7 checks it exists at this rev in `~/.cargo/git/checkouts`, as M11l checked
crates in source); `futures` unbounded channels, `std::process` + `std::thread` (the `pty.rs` / `api_server.rs`
reader pattern); `serde_json`. Claude Code ≥ 2.1.323 and `agy` ≥ 1.4.3 on PATH. **No new crates.**

**Spec:** [[Proposal - Chat View and Agent Tabs]] (Part 1, §1.1–1.8, "AI's calls", "Where it goes").

## Global Constraints

- **No new crates.** `[dependencies]` unchanged. No markdown crate (Task 3 parses a small subset itself).
- **Work in a git worktree on branch `m11aa-agent-tabs`** (`superpowers:using-git-worktrees`): another agent works
  on `master` in the main checkout. Commit per task on the branch; merging is the user's call.
- **Claude argv, exactly:** `claude -p --input-format stream-json --output-format stream-json --verbose
  --include-partial-messages --permission-prompt-tool stdio` + `--resume <id>` when reopening + any extra preset
  arguments (Task 2's pass-through rule). **Before the first user message** fuxx writes
  `{"type":"control_request","request_id":"fuxx-init","request":{"subtype":"initialize","hooks":null}}`.
  Verified 2026-11-13: **without** the initialize request (`--permission-prompts host` alone) Claude asks no one
  and denies every prompt (`system/permission_denied`).
- **agy argv:** `agy -p --input-format stream-json --output-format stream-json --mode <mode>` +
  `--conversation <id>` when reopening. Modes `accept-edits` (default) | `auto` | `plan`. **Never
  `--dangerously-skip-permissions`.**
- **Agents run on this Mac only**, as children of the app (not the background service). Spawned with the login
  shell's `PATH` (`workspace_ssh::login_env()`, background thread only — it can block 3 s), `setsid`, piped stdio;
  closing the tab, the workspace or the app kills the process group.
- **Status mapping (existing `Status`, no new variant):** pending permission → `Waiting`; turn running → `Working`;
  anything said → `Done`; nothing yet → `Idle`. Process exit ≠ 0 or spawn failure → `Done` + an error row.
- **Restore:** only `SavedTab::Agent` restores as a chat. `SavedSession.agent`, `agents_now`,
  `presets::agent_for` and `restore::launch` are **not changed**: a classic `claude` typed in a shell restores
  exactly as today.
- **Scripting ids:** agent tabs are `aN` (`N` from a per-run counter starting at 1), never `sN`.
- **Built-in presets become** `claude`, `agy`, `opencode`, `codex` (⌃⌘1–4). A preset has a provider when its
  **name** is a registered provider's name.
- **Tests:** inline `#[cfg(test)] mod tests` per module. Fixtures under `src/providers/fixtures/` loaded with
  `include_str!`. Gate for every task:
  `cargo test --bin fuxx && cargo clippy --all-targets && cargo fmt --check` (clippy: no new warnings over the
  baseline recorded in Task 0). GUI: `cargo run --features ui-shots -- __ui-shots <scene>`.
- **Debug builds** keep using `sessions-dev.json` (`restore::state_path`).

## Plan-time calls (AI's, made while reading the code; Task 0 copies them into the spec)

1. **"+ adds an agent tab" becomes the pane `···` menu:** the tab bar's `+` has no menu today
   (`BarButton::NewTerminal`); `PaneAction` gains one "New ⟨Label⟩ tab" item per provider (and the ⌘K palette lists
   them). `+` stays "New terminal tab".
2. **Closing a busy agent tab asks** (Working or Waiting). The spec said "like closing a busy terminal today", but
   terminals never ask (`close_tab`: "Never asks"); this is new behaviour, for agent tabs only.
3. **A banner for an agent tab has no click target** (`banner_about(None)`): clicking it brings fuxx forward
   without focusing the tab. Click-to-tab needs `native_notify::Terminal` widened; left for later.
4. **The sidebar row's agent label** for a workspace with an agent tab is the provider's label ("Claude"),
   taken from the agent tab — `agents_now` is never written for it (that would make "agents" restore re-type
   `claude` into the shell).
5. **Long conversations** render through `gpui::list` (only visible items are laid out). Tool output kept per row
   is the **last 2,000 lines / 256 KiB**, with a "… N earlier lines" row.
6. **agy's mode** is chosen on the composer chip **before the first message** and saved with the tab; after that
   the chip is read-only (changing it would restart the process).
7. **Resume history:** if `--resume` doesn't replay earlier messages on stdout (Task 0 checks), `claude.rs` reads
   the conversation's transcript file (path found in Task 0) for the history; if neither works, the tab shows a
   "Conversation restored" notice and only new messages.

## Review Focus

1. **The agent can't start** (not installed, not on the login PATH, not logged in, exits at once): the tab shows
   one error row with the agent's stderr tail and "Open in classic view", status `Done`, no hang, no panic.
   → Task 4 (`missing_program_is_spawn_failed`, `exit_code_and_stderr_reported`), Task 7 (scene `agent-tab-failed`).
2. **Malformed or unexpected output** (a non-JSON line, an unknown `type`, a JSON line split across reads, a line
   over 8 MiB): ignored or reported as one notice, never a crash, the conversation continues.
   → Task 2 (`decode_ignores_unknown_and_garbage`), Task 4 (`lines_survive_split_reads`, `overlong_line_dropped`).
3. **Closing or quitting while the agent works or waits for permission:** confirmation for the tab; after it, no
   `claude` / `agy` process is left running (process group killed, child reaped).
   → Task 4 (`kill_ends_process_group`), Task 8 (close-confirm path).
4. **Restoring a conversation that can't be resumed** (transcript deleted, id unknown): the tab says so in one
   notice row and starts a fresh conversation in the same tab; the session file still loads.
   → Task 2 (`resume_failure_maps_to_notice`), Task 5 (`unknown_provider_restores_as_error_tab`).
5. **Huge tool output** (a Bash command printing 100k lines): the row shows a one-line summary, the expanded
   output keeps only the tail (call 5), the UI stays responsive.
   → Task 3 (`tool_output_is_capped`), Task 7 (scene `agent-tab-long`).

---

### Task 0: Put M11aa in the plan, record the fixtures (go/no-go)

**Files:**
- Modify: `docs/01-Overview/12 - Scope.md`, `docs/04-Development/40 - Roadmap.md`,
  `docs/06-Progress-Log/60 - Progress Log.md`, `docs/04-Development/Proposal - Chat View and Agent Tabs.md`
- Create: `src/providers/fixtures/claude-text.jsonl`, `claude-tools.jsonl`, `claude-permission-allow.jsonl`,
  `claude-permission-deny.jsonl`, `claude-permission-always.jsonl`, `claude-interrupt.jsonl`,
  `claude-resume.jsonl`, `claude-slash.jsonl`, `claude-bad-resume.jsonl`, `agy-text.jsonl`, `agy-tools.jsonl`,
  `agy-resume.jsonl`
- Create: `scripts/record-agent-fixture.py` (the recorder; stdlib only, run with `python3 -I`)

- [ ] **Step 1:** Only when the user says the `master` agent is between tasks: add M11aa (Part 1) and M11ab (Part 2,
  "design first") to Scope and Roadmap after M11p, add a Progress Log entry, and in the proposal flip the warning
  box to "In the plan as M11aa / M11ab", copy in "Plan-time calls", and correct §1.6's "like closing a busy terminal
  today" (call 2) and §1.4's `+` (call 1).
- [ ] **Step 2:** Record the clippy warning count on the branch base: `cargo clippy --all-targets 2>&1 | grep -c '^warning'`
  → write it in this plan's "At a glance".
- [ ] **Step 3:** Write `scripts/record-agent-fixture.py <agent> <scenario> <workdir> <out>`: starts the argv from
  Global Constraints in `<workdir>` (a fresh temp dir), sends the initialize request (Claude only), sends the
  scenario's messages, answers `can_use_tool` requests per scenario (`allow` / `deny` with message
  `"Not now"` / `allow` + `updatedPermissions` = the request's `permission_suggestions`), and writes every stdout
  line to `<out>`. Scenarios: `text` ("Say hello in one word"), `tools` (Read a file, Edit it, run `ls` with Bash,
  write a TodoWrite list — use a prompt that asks for all four), `permission-allow|deny|always` (Write `hello.txt`),
  `interrupt` (a long task, then `{"type":"control_request","request_id":"fuxx-stop","request":{"subtype":"interrupt"}}`
  after the first tool starts), `resume` (`--resume <id from text>`, one more message), `slash` (send the text
  `/compact` as a user message), `bad-resume` (`--resume 00000000-0000-0000-0000-000000000000`).
  Allow response shape, verified 2026-11-13:
  `{"type":"control_response","response":{"subtype":"success","request_id":<id>,"response":{"behavior":"allow","updatedInput":<input>}}}`.
  The user's own hooks run in these sessions (same harness); a hook that blocks a tool is part of what the
  fixtures should show, not something to switch off.
- [ ] **Step 4:** Run every scenario for Claude, then for agy (skip agy's permission scenarios). Scrub absolute
  temp paths to `/tmp/fx` and the user's home to `~` in the files.
- [ ] **Step 5: Go/no-go.** Write into the proposal's "Claude Code headless mode" section what each recording
  shows, answering: does `deny` with a message reach the model; does `updatedPermissions` make the next identical
  Write skip the prompt; does `interrupt` stop the turn and what arrives after it; can a second user message be
  sent while a turn runs (and what `queued_turn_count` says); does `--resume` replay history on stdout, and if not,
  where the transcript file is; does `/compact` run; what `bad-resume` prints; where context-window size appears
  (`result.modelUsage`?) for the `context N%` line; agy's event names for text, tools, conversation id, turn end.
  **If allow or deny doesn't work, stop and report to the user** (the proposal's gate). Otherwise continue.
- [ ] **Step 6: Commit** — `git add docs scripts/record-agent-fixture.py src/providers/fixtures && git commit -m "M11aa: plan, scope and agent fixtures"`

### Task 1: Presets know about providers

**Files:** Modify `src/presets.rs`. Test: inline.

**Interfaces:**
- Produces: `presets::builtins()` → claude, agy, opencode, codex; `pub fn extra_args(preset: &Preset) -> Vec<String>`
  — the preset command's words after the first, split on whitespace. (`provider_for` arrives in Task 2, once
  `providers` exists.)

- [ ] **Step 1: Failing tests**

```rust
#[test] fn builtins_are_claude_agy_opencode_codex() {
    let names: Vec<_> = builtins().into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["claude", "agy", "opencode", "codex"]);
}
#[test] fn extra_args_are_words_after_the_program() {
    let p = Preset { name: "claude".into(), command: "claude --model sonnet".into() };
    assert_eq!(extra_args(&p), ["--model", "sonnet"]);
}
```
(Replace the old `builtins_are_claude_opencode_codex`.)

- [ ] **Step 2:** `cargo test --bin fuxx presets` → FAIL. **Step 3:** implement. **Step 4:** → PASS; fix any other
  test that counted three built-ins (`grep -rn "builtins()" src`). **Step 5: Commit** `M11aa: agy built-in preset`.

### Task 2: Providers — the event language and Claude's translator

**Files:** Create `src/providers/mod.rs`, `src/providers/claude.rs`; modify `src/main.rs` (`mod providers;`),
`src/presets.rs` (`provider_for`). Test: inline, against Task 0's fixtures.

**Interfaces:**
- Produces (`providers/mod.rs`, GPUI-free, no agent names in it):

```rust
pub struct ProviderInfo {
    pub name: &'static str,          // "claude" — matches a preset name
    pub label: &'static str,         // "Claude"
    pub badge: &'static str,         // "CC"
    pub badge_rgb: u32,              // 0x7a4a2a for Claude, 0x2d4f8a for agy (mockups)
    pub routes_permissions: bool,
    pub modes: &'static [&'static str],      // [] for Claude
    pub default_mode: Option<&'static str>,
}
pub struct Start { pub cwd: PathBuf, pub resume: Option<String>, pub mode: Option<String>, pub extra_args: Vec<String> }
pub trait Provider: Sync {
    fn info(&self) -> &'static ProviderInfo;
    fn argv(&self, start: &Start) -> Vec<String>;
    fn classic_argv(&self, conversation: &str) -> Vec<String>;   // for "Open in classic view"
    fn history(&self, start: &Start) -> Vec<Event> { Vec::new() } // plan-time call 7
    fn translator(&self) -> Box<dyn Translator>;
}
pub trait Translator: Send {
    fn hello(&mut self) -> Vec<String>;                  // lines written before the first message
    fn decode(&mut self, line: &str) -> Vec<Event>;      // one stdout line → zero or more events
    fn encode(&mut self, action: &Action) -> Vec<String>;
}
pub enum Event {
    Started { conversation: String, model: Option<String> },
    TextDelta { message: String, text: String },
    MessageDone { message: String, text: String },
    ToolStarted { id: String, name: String, summary: String, parent: Option<String>, detail: Option<ToolDetail> },
    ToolFinished { id: String, ok: bool, summary: String, output: Option<String> },
    Todo(Vec<TodoItem>),
    Permission { id: String, tool: String, what: String, detail: String, always: Option<String> },
    Usage { context_pct: Option<u8>, cost_usd: Option<f64> },
    TurnDone { ok: bool, message: Option<String> },
    Notice(String),
    Error(String),
}
pub enum ToolDetail { Diff { path: String, unified: String }, Command(String) }
pub struct TodoItem { pub text: String, pub state: TodoState }
pub enum TodoState { Done, Active, Pending }
pub enum Action { Send { text: String }, Allow { id: String }, AlwaysAllow { id: String }, Deny { id: String, reason: Option<String> }, Interrupt }
pub fn all() -> &'static [&'static dyn Provider];        // [&claude::CLAUDE] now; agy added in Task 12
pub fn by_name(name: &str) -> Option<&'static dyn Provider>;
// presets.rs
pub fn provider_for(preset: &Preset) -> Option<&'static dyn crate::providers::Provider>;   // by_name(&preset.name)
```
(Derive `Clone, Debug, PartialEq` on the data types; `f64` blocks `Eq`.)

- `claude.rs`: `pub static CLAUDE: Claude;` implementing `Provider`. Translator mapping (✓ = seen in a 2026-11-13
  recording; others confirmed by Task 0's fixtures):
  - ✓ `system/init` → `Started { conversation: session_id, model }`.
  - ✓ `stream_event` / `content_block_delta` / `text_delta` → `TextDelta { message: api_message_id, text }`;
    `thinking_delta`, `signature_delta`, `input_json_delta` → nothing.
  - ✓ `assistant` → per content item: `text` → `MessageDone`; `tool_use` → `ToolStarted` (summary from input:
    `file_path` file name, else `command` first line, else `pattern`, else the tool name; detail: Edit →
    `Diff` built from `old_string`/`new_string`, Write → `Diff` of all-added `content`, Bash → `Command`);
    `TodoWrite` → `Todo` from `input.todos[{content,status}]`; `thinking` → nothing. `parent` =
    `parent_tool_use_id`.
  - ✓ `user` with `tool_result` → `ToolFinished { id: tool_use_id, ok: !is_error, summary, output }` (summary:
    first line, ≤ 60 chars; a hook's block message counts as `ok: false`).
  - ✓ `control_request` / `can_use_tool` → `Permission { id: request_id, tool: display_name, what: description
    (or the command for Bash), detail: input pretty-printed, always: first permission_suggestion described in
    words }`; the translator keeps `input` and `permission_suggestions` by id for `encode`.
  - ✓ `system/permission_denied` → `Notice(message)`; ✓ `result` → `Usage { cost_usd: total_cost_usd, .. }` then
    `TurnDone { ok: !is_error, message }`; context % from wherever Task 0 found it, else `None`.
  - `system/hook_response` with a non-zero exit → `Notice` (field name from fixtures); ✓ `system/status`,
    `system/thinking_tokens`, `system/hook_started`, `rate_limit_event`, `control_response` → nothing.
  - Anything unparsable or unknown → nothing.
  - `encode`: `Send` → `{"type":"user","message":{"role":"user","content":text}}`; `Allow` → the verified
    response with the kept `updatedInput`; `AlwaysAllow` → same plus `"updatedPermissions"`: the kept suggestions;
    `Deny` → `{"behavior":"deny","message": reason or "The user denied this."}`; `Interrupt` → the interrupt
    control request. `hello` → the initialize request.
  - Pass-through (`argv`): `extra_args` are appended except `-p`, `--print`, `--input-format`, `--output-format`,
    `--permission-prompt-tool`, `--permission-prompts`, `--resume`, `-r`, `--continue`, `-c` and their values.

- [ ] **Step 1: Failing tests** (in `claude.rs`; `decode_all(p, text)` is a test helper feeding each line):

```rust
#[test] fn text_fixture_streams_then_finishes() {
    let ev = decode_all(&CLAUDE, include_str!("fixtures/claude-text.jsonl"));
    assert!(matches!(ev.first(), Some(Event::Started { .. })));
    assert!(ev.iter().any(|e| matches!(e, Event::TextDelta { .. })));
    assert!(matches!(ev.last(), Some(Event::TurnDone { ok: true, .. })));
}
#[test] fn tools_fixture_pairs_every_start_with_a_finish() { /* each ToolStarted id has one ToolFinished; Edit has Diff; one Todo */ }
#[test] fn permission_request_round_trips() {
    // decode the can_use_tool line from claude-permission-allow.jsonl → Permission{id,..};
    // encode(Allow{id}) → one line whose response.behavior == "allow" and updatedInput == the request's input
    // encode(AlwaysAllow{id}) → also has updatedPermissions == the request's permission_suggestions
    // encode(Deny{id, reason: Some("Not now")}) → behavior "deny", message "Not now"
}
#[test] fn decode_ignores_unknown_and_garbage() {
    let mut t = CLAUDE.translator();
    assert!(t.decode("not json").is_empty());
    assert!(t.decode(r#"{"type":"brand_new_thing"}"#).is_empty());
    assert!(t.decode(r#"{"type":"assistant"}"#).is_empty());
}
#[test] fn resume_failure_maps_to_notice() { /* claude-bad-resume.jsonl → an Error or Notice, never a panic */ }
#[test] fn hello_is_the_initialize_request() { /* one line; request.subtype == "initialize" */ }
#[test] fn argv_drops_flags_fuxx_owns() {
    let s = Start { cwd: "/tmp".into(), resume: None, mode: None,
        extra_args: vec!["--model".into(), "sonnet".into(), "-p".into(), "--resume".into(), "x".into()] };
    let a = CLAUDE.argv(&s);
    assert!(a.ends_with(&["--model".to_string(), "sonnet".to_string()]));
    assert_eq!(a.iter().filter(|w| *w == "-p").count(), 1);
    assert!(!a.contains(&"x".to_string()));
}
```
plus in `mod.rs`: `by_name("claude")` is Claude, `by_name("opencode")` is None; in `presets.rs`:
`provider_for` is Some for claude, None for codex.

- [ ] **Step 2:** `cargo test --bin fuxx providers` → FAIL. **Step 3:** implement. **Step 4:** → PASS.
- [ ] **Step 5: Commit** `M11aa: providers and the Claude translator`.

### Task 3: The conversation model and message markdown (pure)

**Files:** Create `src/agent_conversation.rs`, `src/agent_markdown.rs`. Test: inline.

**Interfaces:**
- Consumes: `providers::{Event, ToolDetail, TodoItem}`, `status::Status`.
- Produces:

```rust
// agent_conversation.rs
pub enum Item {
    You { text: String, attachments: Vec<String> },
    Agent { message: String, text: String, done: bool },
    Tool { id: String, name: String, summary: String, parent: Option<String>, running: bool, ok: Option<bool>,
           result: String, detail: Option<ToolDetail>, output: Option<String>, dropped_lines: usize },
    Todo(Vec<TodoItem>),
    Permission { id: String, tool: String, what: String, detail: String, always: Option<String>, answer: Option<Answer> },
    Notice(String),
    Error(String),
}
pub enum Answer { Allowed, AlwaysAllowed, Denied(Option<String>) }
pub struct Conversation { /* items, conversation id, model, cost, context %, turn_running, ended */ }
impl Conversation {
    pub fn new() -> Self;
    pub fn apply(&mut self, event: Event);
    pub fn you(&mut self, text: String, attachments: Vec<String>);   // also sets turn_running
    pub fn answered(&mut self, id: &str, answer: Answer);
    pub fn process_ended(&mut self, code: Option<i32>, stderr_tail: &str);
    pub fn items(&self) -> &[Item];
    pub fn pending_permission(&self) -> Option<&Item>;
    pub fn status(&self) -> Status;
    pub fn conversation(&self) -> Option<&str>;
    pub fn model(&self) -> Option<&str>;
    pub fn cost_usd(&self) -> Option<f64>;
    pub fn context_pct(&self) -> Option<u8>;
    pub fn title(&self) -> Option<String>;   // first line of the first You, ≤ 40 chars + "…"
}
pub const OUTPUT_KEEP_LINES: usize = 2_000;
pub const OUTPUT_KEEP_BYTES: usize = 256 * 1024;

// agent_markdown.rs
pub enum Md { Para(Vec<Span>), Heading(u8, Vec<Span>), Bullet(Vec<Span>), Numbered(u32, Vec<Span>), Code { lang: Option<String>, text: String } }
pub enum Span { Text(String), Code(String), Bold(String), Link { text: String, url: String } }
pub fn parse(text: &str) -> Vec<Md>;
```
Folding rules: `TextDelta` appends to (or creates) the `Agent` item with that `message`; `MessageDone` replaces its
text and marks it done; `ToolFinished` updates the `Tool` with that id; a later `Todo` replaces the earlier `Todo`
item in place; a `Permission` stays pending until `answered`; `TurnDone` clears `turn_running`.

- [ ] **Step 1: Failing tests**

```rust
#[test] fn status_follows_the_turn() {
    let mut c = Conversation::new();
    assert_eq!(c.status(), Status::Idle);
    c.you("hi".into(), vec![]);                            assert_eq!(c.status(), Status::Working);
    c.apply(permission("p1"));                             assert_eq!(c.status(), Status::Waiting);
    c.answered("p1", Answer::Allowed);                     assert_eq!(c.status(), Status::Working);
    c.apply(Event::TurnDone { ok: true, message: None });  assert_eq!(c.status(), Status::Done);
}
#[test] fn deltas_build_one_message() { /* two TextDelta "Hel","lo" same message → one Agent item "Hello" */ }
#[test] fn todo_updates_in_place() { /* two Todo events → one Todo item, the second's contents */ }
#[test] fn tool_output_is_capped() {
    // ToolFinished with 100_000 lines → output has OUTPUT_KEEP_LINES lines (the last ones), dropped_lines == 98_000
}
#[test] fn crash_is_done_with_an_error() {
    let mut c = Conversation::new(); c.you("x".into(), vec![]);
    c.process_ended(Some(1), "not logged in");
    assert_eq!(c.status(), Status::Done);
    assert!(matches!(c.items().last(), Some(Item::Error(e)) if e.contains("not logged in")));
}
#[test] fn replays_the_tools_fixture() { /* every Tool item ends with running == false */ }
// agent_markdown.rs
#[test] fn parses_the_subset() {
    let md = parse("Hi `x` **b**\n\n- one\n1. two\n```rust\nfn f(){}\n```\n[a](https://e.x)");
    // Para[Text("Hi "),Code("x"),Text(" "),Bold("b")], Bullet, Numbered(1,..), Code{lang:Some("rust"),..}, Para[Link]
}
#[test] fn unterminated_fence_is_code_to_the_end() { /* "```\nabc" → [Code{lang:None,text:"abc"}] (streaming) */ }
```

- [ ] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** → PASS. **Step 5: Commit** `M11aa: conversation model and markdown`.

### Task 4: Running the agent process

**Files:** Create `src/agent_process.rs`. Test: inline, with `/bin/sh -c` stand-ins for the agent.

**Interfaces:**
- Produces:

```rust
pub enum ProcessEvent { Line(String), Exited { code: Option<i32>, stderr_tail: String }, SpawnFailed(String) }
pub struct AgentProcess { /* pid, writer Sender<String> */ }
pub const MAX_LINE: usize = 8 * 1024 * 1024;
pub fn spawn(argv: &[String], cwd: &Path, path_env: Option<&str>,
             events: futures::channel::mpsc::UnboundedSender<ProcessEvent>) -> AgentProcess;
impl AgentProcess { pub fn send(&self, line: String); pub fn kill(&self); }
impl Drop for AgentProcess { /* kill */ }
```
`spawn` never blocks the UI: it starts a named thread `fuxx-agent` that spawns the child (`Stdio::piped()` ×3,
`pre_exec(setsid)` as in `git_branch::run_capped_streams`, `PATH` = `path_env` when given), a reader thread
(`BufRead::read_line` loop; a line over `MAX_LINE` is skipped and sent as one `Line` of `{"type":"fuxx_overlong"}`
which every translator ignores), a stderr thread keeping the last 4 KiB, and a writer thread draining an
`std::sync::mpsc` of lines (each written with `\n`, then flushed). Spawn errors arrive as `SpawnFailed`. `kill`
sends SIGTERM to the process group, then SIGKILL after 2 s, and reaps.

- [ ] **Step 1: Failing tests** (each collects events with `futures::executor::block_on` + a 5 s timeout)

```rust
#[test] fn echoes_lines_back() { /* argv ["/bin/sh","-c","cat"]; send("a"); send("b") → Line("a"), Line("b") */ }
#[test] fn lines_survive_split_reads() { /* sh -c 'printf "{\"x\":"; sleep 0.2; printf "1}\n"' → one Line("{\"x\":1}") */ }
#[test] fn overlong_line_dropped() { /* a 9 MiB line then "ok" → the overlong marker, then Line("ok") */ }
#[test] fn exit_code_and_stderr_reported() { /* sh -c 'echo nope >&2; exit 3' → Exited{code:Some(3), stderr_tail contains "nope"} */ }
#[test] fn missing_program_is_spawn_failed() { /* argv ["/nonexistent/agent"] → SpawnFailed(_) */ }
#[test] fn kill_ends_process_group() { /* sh -c 'sleep 100 & sleep 100'; kill(); after 3 s `kill(-pgid, 0)` fails with ESRCH */ }
```

- [ ] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** → PASS. **Step 5: Commit** `M11aa: agent process runner`.

### Task 5: `TabKind::Agent` in the layout model and restore

**Files:** Modify `src/layout_model.rs` (`TabKind` `:27`, `SavedTab` `:189`, `to_saved` `:805`, `restored_pane`
`:1036`, `saved_focus` `:1163`, `SavedFocus` `:1177`, `shown_elsewhere` `:1086`), and every exhaustive match:
`adopt.rs:426,438,963`, `palette.rs:1008,1036`, `tab_bar.rs:94,127,760`, `file_tab.rs:3637`, `sidebar.rs:1312`,
`workspace.rs:1698`, `workspace_browser.rs:381` (re-grep `TabKind::File` first; line numbers are from 2026-11-13
and the code moves). Test: inline in `layout_model.rs`.

**Interfaces:**
- Produces:

```rust
pub enum TabKind { Terminal(u64), Browser { .. }, File { .. },
    Agent { provider: String, conversation: Option<String>, mode: Option<String>, title: Option<String> } }
pub enum SavedTab { .., Agent { provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] conversation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] title: Option<String> } }   // JSON kind "agent"
impl Model { pub fn agent_tabs(&self, ws: WorkspaceId) -> Vec<TabId>; }
```
`SAVED_VERSION` stays 2 (as File did; an older fuxx can't read a file with an agent tab — same accepted limit as
File tabs). Agent tabs move between panes like File tabs and are never shown twice (`shown_elsewhere` treats them
like Terminal). New match arms elsewhere do the minimum to compile and behave like a non-terminal (no session id);
Tasks 8–10 fill them in.

- [ ] **Step 1: Failing tests**

```rust
#[test] fn agent_tab_round_trips_through_saved() {
    // new_workspace with Terminal(1), split with Agent{provider:"claude",conversation:Some("c1"),mode:None,title:Some("fix")}
    // to_saved → JSON contains "\"kind\":\"agent\"" and "\"conversation\":\"c1\""; from_saved → same kinds back
}
#[test] fn old_files_without_agent_tabs_still_load() { /* a v2 JSON with only terminal/file tabs decodes unchanged */ }
#[test] fn unknown_provider_restores_as_error_tab() { /* SavedTab::Agent{provider:"gone"} restores as TabKind::Agent{provider:"gone",..}; Task 7's view shows the error */ }
#[test] fn agent_tabs_lists_only_agents() { /* workspace with Terminal + Agent + File → agent_tabs == [that tab] */ }
```

- [ ] **Step 2:** → FAIL. **Step 3:** implement; `cargo build` until every match compiles. **Step 4:** → PASS (whole
  suite). **Step 5: Commit** `M11aa: agent tabs in the layout model`.

### Task 6: A multi-line composer input

**Files:** Create `src/composer.rs`; modify `src/main.rs` (key bindings, context `"Composer"`). Model it on
`src/name_input.rs` (`EntityInputHandler`, IME, selection, its actions). Test: inline for the pure editing core.

**Interfaces:**
- Produces:

```rust
pub enum ComposerEvent { Send(String), Stop, AtTyped }   // Send: Enter (text may be empty — the view decides); Stop: Esc
pub struct Composer;  // impl EventEmitter<ComposerEvent>, Focusable, EntityInputHandler, Render
impl Composer {
    pub fn new(placeholder: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self;
    pub fn text(&self) -> &str; pub fn set_text(&mut self, text: String, cx: &mut Context<Self>);
    pub fn insert(&mut self, text: &str, cx: &mut Context<Self>);
    pub fn set_placeholder(&mut self, p: SharedString, cx: &mut Context<Self>);
}
// pure core, unit-tested:
pub struct EditBuffer { /* text, cursor, anchor */ }
impl EditBuffer { pub fn insert(&mut self, s: &str); pub fn newline(&mut self); pub fn backspace(&mut self);
                  pub fn up(&mut self); pub fn down(&mut self); pub fn rows(&self) -> usize; }
```
Unlike `NameInput`, blur does **not** cancel. Enter = Send, ⇧Enter = newline, Esc = Stop, ⌘A / ⌘C / ⌘V / ⌘X /
⌥← / ⌥→ / ⌘← / ⌘→ as `NameInput`. Grows to 8 rows, then scrolls. Typing `@` at a word start emits `AtTyped`.

- [ ] **Step 1: Failing tests** — `EditBuffer`: `newline_then_up_keeps_column`, `backspace_joins_lines`,
  `rows_counts_newlines` (`"a\nb\nc"` → 3), `insert_replaces_selection`.
- [ ] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** → PASS. **Step 5: Commit** `M11aa: multi-line composer`.

### Task 7: The agent tab view

**Files:** Create `src/agent_tab.rs`; modify `src/ui_shots.rs` (scenes). Check `gpui::list` / `ListState` at the
pinned rev in `~/.cargo/git/checkouts/zed-*/1d2a4b3f*/crates/gpui` before using it; if absent, a scrolled `div`
with the caps from plan-time call 5 is the fallback (say so in the commit).

**Interfaces:**
- Consumes: Tasks 2, 3, 4, 6.
- Produces:

```rust
pub enum AgentTabEvent { Status(Status), Title(String), Conversation(String), Close, OpenClassic(Vec<String>) }
pub struct AgentTab;   // EventEmitter<AgentTabEvent>, Focusable, Render
impl AgentTab {
    pub fn new(provider: Option<&'static dyn Provider>, provider_name: String, start: Start,
               window: &mut Window, cx: &mut Context<Self>) -> Self;   // None provider → error tab
    pub fn send(&mut self, text: String, cx: &mut Context<Self>);
    pub fn attach(&mut self, attachment: String, cx: &mut Context<Self>);
    pub fn answer(&mut self, answer: Answer, cx: &mut Context<Self>) -> Result<(), &'static str>; // Err "not waiting"
    pub fn stop(&mut self, cx: &mut Context<Self>);
    pub fn status(&self) -> Status;
    pub fn question(&self) -> Option<String>;   // "Claude wants to run: <what>"
    pub fn mode(&self) -> Option<&str>;
    pub fn activity(&self) -> &ActivityLog;
}
```
Layout, from the mockup `c1-agent-tab.html` and spec §1.1–1.2, colours from `theme::UiColors` only:
- Top line: status word + `context N%` when known (`text::META`, `text_dim`).
- Items: `You` right-aligned on `card` with `card_border`, attachments as chips; `Agent` = badge avatar + markdown
  (`agent_markdown::parse`, code blocks on `panel` in the terminal font); `Tool` row on `panel` with tool name in
  `accent`, summary, result at right, a spinner + elapsed seconds while running; click toggles detail (Diff through
  `diff_view::parse_diff` + the file tab's row colours; Command/output in the terminal font, with the dropped-lines
  row); sub-agent tools nested under their parent, collapsed by default; `Todo` as ✓ / ◐ / ○ rows; `Permission`
  card with `ring_waiting` border, the `what` and `detail`, buttons **Allow ⏎**, **Always allow ⟨always⟩** (only
  when `always` is Some), **Deny**; answered cards collapse to one line ("Allowed", "Denied: reason");
  `Notice` in `text_dim`; `Error` in `block_failed` with **Open in classic view** when a conversation id exists.
- Composer: mode chip (agy, before the first message, call 6), model chip, cost chip, hints
  "/ commands · @ files"; while Working the right button is **■ Stop (esc)** (sends `Action::Interrupt`); with a
  pending permission the placeholder is "Deny and say why… (optional)", Enter on an empty composer = Allow, Enter
  with text = Deny with that reason. Messages sent while Working go straight to the process if Task 0 showed
  Claude queues them, else are held and sent on `TurnDone`.
- `@`: a popup listing workspace files filtered with `palette_match` over `palette_files`' list for `start.cwd`;
  Enter inserts the relative path.
- Empty state: "Ask ⟨Label⟩ to do something in ⟨~/short/cwd⟩", composer focused.
- On new: spawn (Task 4) with `provider.argv(&start)` on a background task that first reads
  `workspace_ssh::login_env()`'s PATH; write `translator.hello()`; render `provider.history(&start)` first.
  Events go `ProcessEvent::Line` → `translator.decode` → `Conversation::apply`; emit `Status` when
  `conversation.status()` changes, `Title` on the first `You`, `Conversation` on `Started`. A resume that fails
  (Review Focus 4) adds a notice and respawns without `--resume`.

- [ ] **Step 1:** Add ui-shots scenes driven by fixture events (no process): `agent-tab-empty`, `agent-tab-working`
  (tools fixture mid-way), `agent-tab-permission`, `agent-tab-done`, `agent-tab-failed` (`process_ended(Some(1),
  "not logged in")`), `agent-tab-long` (300 items plus a 100k-line tool output, scrolled to the end). Run
  `cargo run --features ui-shots -- __ui-shots agent-tab` → fails (no view yet).
- [ ] **Step 2:** Implement `AgentTab`. **Step 3:** scenes render; compare each PNG with the mockup; check text
  contrast ≥ `TEXT_MIN` with `ui_tokens::contrast` for `text_dim` on `panel`.
- [ ] **Step 4:** Full gate. **Step 5: Commit** `M11aa: agent tab view`.

### Task 8: Agent tabs in the workspace

**Files:** Create `src/workspace_agent.rs` (pattern: `workspace_browser.rs`); modify `src/workspace.rs`
(`Workspace` fields near `:667`, `render_panes` `:2843`, `focus_active` `:2823`, `sync` `:1592`, `Render` `:3145`,
`launch_preset` `:1975`, `open_local` / `attach_session`), `src/restore.rs` (`Launch` `:240`), `src/welcome.rs`
(`run_welcome_pick`), `src/palette.rs` (`Key::Preset`), `src/tab_bar.rs` (`TabIcon`, `icon`, `bar_tabs`,
`render_tab`, `close_tab`), `src/pane_actions.rs`, `src/pane_tab_menu.rs`, `src/tab_menu.rs` (`busy`),
`src/palette_items.rs`, `src/sidebar.rs` (preset menu `:2590-2630`).

**Interfaces:**
- Consumes: `AgentTab` (Task 7), `TabKind::Agent` (Task 5), `presets::{provider_for, extra_args}` (Tasks 1–2).
- Produces (on `Workspace` unless noted):

```rust
pub(crate) agent_tabs: HashMap<TabId, Entity<AgentTab>>,
pub(crate) agent_ids: HashMap<TabId, u64>, next_agent_id: u64,          // aN, Task 10
pub(crate) fn ensure_agent_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>);
pub(crate) fn pane_agent_tab(&self, pane: PaneId) -> Option<Entity<AgentTab>>;
pub(crate) fn focused_agent_tab(&self) -> Option<(TabId, Entity<AgentTab>)>;
pub(crate) fn new_agent_tab(&mut self, pane: PaneId, provider: &str, window: &mut Window, cx: &mut Context<Self>);
pub(crate) fn close_agent_tab(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>); // asks when Working/Waiting
// workspace_agent.rs, pure:
pub enum LaunchPlan { Classic, ShellThenAgent(AgentStart) }
pub fn launch_plan_for(preset: &Preset, ssh: bool) -> LaunchPlan;
// restore.rs:
pub struct AgentStart { pub provider: String, pub extra_args: Vec<String> }
pub struct Launch { pub saved: SavedSession, pub run_preset: bool, pub then_agent: Option<AgentStart> }
```
- `launch_preset(n)`, the welcome screen's `Choice::Preset` and ⌘K's `Key::Preset`: when `launch_plan_for` says
  `ShellThenAgent`, open the shell **without typing the preset** (`run_preset: false`, `preset: None` on the saved
  session so restore never re-types it) with `then_agent: Some(..)`; once that terminal's tab exists (after
  `insert_session` succeeds, in both `open_local` and `attach_session`), `model.split(its pane, Axis::Row,
  TabKind::Agent{..})`, `sync`, `save`, focus the agent tab. `Classic`: unchanged.
- Tab bar: `TabIcon::Agent { badge: &'static str, rgb: u32 }` drawn as a small rounded badge; the status mark
  follows the title when not Idle; title = `TabKind::Agent.title` or the provider label; tip = title + status.
- `···` menu (`PaneAction`): one "New ⟨Label⟩ tab" per provider, hidden when the pane's workspace has an SSH
  session (local only). ⌘K lists the same.
- Tab context menu: **Rename** works for agent tabs (sets `TabKind::Agent.title`); **Open in classic view** (agent
  tabs with a conversation id) opens a terminal tab in the same pane typing `provider.classic_argv(id)` joined with
  spaces (via `Launch` with a one-off `Preset { name, command }` and `run_preset: true`).
- Sidebar preset menu: provider presets get the badge before the name.
- `AgentTabEvent::Title` / `Conversation` → `model.set_kind` + `save()`; `Close` → `close_agent_tab`;
  `OpenClassic` → as above.
- `tab_menu::busy` and the close-others count include agent tabs that are Working or Waiting.
- Quitting the app (the existing quit path, `quit_after_flush`) calls `AgentProcess::kill` on every agent tab
  before exiting — don't rely on `Drop` running at exit.

- [ ] **Step 1:** Failing pure test `launch_plan_follows_provider_and_ssh` (`claude` local → `ShellThenAgent`,
  `claude` with `ssh: true` → `Classic`, `codex` → `Classic`); add ui-shots scenes `agent-workspace-split`,
  `agent-preset-menu`, `agent-pane-menu`. Run → FAIL.
- [ ] **Step 2:** Implement. **Step 3:** test + scenes pass; manually in a debug build: ⌃⌘1 → shell + Claude side by
  side; quit and relaunch → the agent tab comes back with its conversation and the shell does not type `claude`;
  close a Working agent tab → asked; afterwards `pgrep -f "claude -p"` finds nothing.
- [ ] **Step 4:** Full gate. **Step 5: Commit** `M11aa: agent tabs in the workspace`.

### Task 9: Status, notifications, sidebar, Activity

**Files:** Modify `src/workspace_agent.rs`, `src/workspace.rs` (`set_status` `:2067` as the reference),
`src/layout_model.rs` (`rollup` `:718`), `src/sidebar.rs` (`row_subjects` `:498`, agent label),
`src/activity_panel.rs` (show the focused agent tab's log), `src/status.rs` (`urgency`).

**Interfaces:**
- Produces:
  - `fn set_agent_status(&mut self, tab: TabId, new: Status, cx: &mut Context<Self>)`: for an agent tab, what
    `set_status` does for a terminal — status + since, Activity entry (in `AgentTab`'s log), API event (Task 10),
    the notification list entry, sound + banner via `notifier::{should_notify, message, body, banner_title,
    banner_subtitle, worth_a_banner, fire}` with `about = None` (call 3), `check_waits`, `cx.notify()`.
  - `pub fn urgency(s: Status) -> u8` in `status.rs`, with the order `Model::rollup` uses today (and `rollup` then
    uses it).
  - `pub fn row_status(terminal: Option<Status>, agents: &[Status]) -> Status` in `workspace_agent.rs` (the most
    urgent); `row_subjects` uses it. Sidebar agent label = the provider label for a workspace with an agent tab
    (call 4).
- [ ] **Step 1: Failing tests:** `urgency_orders_like_rollup` (status.rs); `waiting_agent_beats_idle_shell`
  (`row_status(Some(Idle), &[Waiting]) == Waiting`), `no_agents_is_terminal_status`.
- [ ] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** scene `agent-sidebar-waiting` (red ring on the row);
  manual: a permission prompt while another workspace is focused → ring, 🔔 entry, banner. Full gate.
- [ ] **Step 5: Commit** `M11aa: agent tab status, notifications and sidebar`.

### Task 10: Scripting API and MCP see agent tabs

**Files:** Modify `src/api.rs` (`Target` `:109`, `parse_target`), `src/api_events.rs` (`Watch`, wait state, event
session JSON), `src/workspace_api.rs` (`session.list`, `.status`, `.question`, `.answer`, `.wait`, `statuses()`,
`emit`), `src/mcp.rs` (tool descriptions mention `aN`).

**Interfaces:**
- Produces: `Target::Agent(u64)` parsed from `aN`; `pub enum Subject { Terminal(u64), Agent(u64) }` with
  `Display` `sN` / `aN`; `Watch::One(Subject)`; `statuses() -> Vec<(Subject, Status)>` including agent tabs;
  `pub fn answer_action(text: &str) -> Answer` in `api.rs` (`y`/`yes`/`allow` → Allowed, `always` →
  AlwaysAllowed, else Denied(Some(text))).
  `session.list` rows for agent tabs: `{"id":"aN","kind":"agent","provider":"claude","name":<title>,"status":..}`
  (terminal rows gain `"kind":"terminal"`). `session.question` on `aN` → `{"text": AgentTab::question()}` (error
  `not_waiting` when none). `session.answer` on `aN` → `AgentTab::answer(answer_action(text))`. `send`, `key`,
  `paste`, `read` on `aN` → error `"aN is an agent tab, not a terminal"`. No-target resolution is unchanged.
- [ ] **Step 1: Failing tests** (pure): `parse_target("a3") == Ok(Target::Agent(3))` and `"s3"` unchanged;
  `answer_words_map_to_actions`; `wait_any_sees_agent_waiting` (a `Watch::Any` wait for `waiting` is met when
  `statuses` gains `(Subject::Agent(1), Waiting)`).
- [ ] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** manual in a debug build: `fuxx status a1`;
  `fuxx wait --any --for waiting` returns when the agent asks; `fuxx answer a1 yes` allows it. Full gate.
- [ ] **Step 5: Commit** `M11aa: agent tabs in the scripting API`.

### Task 11: Drag a command block onto an agent tab

**Files:** Modify `src/block_view.rs` / `src/terminal_view.rs` (the M11l chip), `src/agent_tab.rs`,
`src/tab_bar.rs` (an agent tab in the bar is a drop target too).

**Interfaces:**
- Produces: `pub struct BlockDrag { pub command: String, pub exit: Option<i32>, pub output: String }` (in
  `block_view.rs`); `pub fn attachment_text(d: &BlockDrag) -> String` — `"$ {command}\n(exit {exit})\n{output}"`,
  the output's tail capped at 64 KiB with a first line `… (earlier output cut)`, no exit line when `exit` is None.
  The chip's command text is the drag handle (GPUI `on_drag`, as tabs already use); the agent tab body and its
  bar tab accept it (`on_drop::<BlockDrag>` → `AgentTab::attach`). Paste into the composer is unchanged.
- [ ] **Step 1: Failing test:** `attachment_text_formats_and_caps` (100 KiB output → ≤ 64 KiB + the cut line;
  `exit: None` → no exit line). **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** scene
  `agent-tab-attachment`; manual drag in a debug build. Full gate.
- [ ] **Step 5: Commit** `M11aa: attach a command block to an agent`.

### Task 12: The agy provider

**Files:** Create `src/providers/agy.rs`; modify `src/providers/mod.rs` (`all()`).

**Interfaces:**
- Produces: `pub static AGY: Agy;` — `ProviderInfo { name: "agy", label: "Antigravity", badge: "AG", badge_rgb:
  0x2d4f8a, routes_permissions: false, modes: &["accept-edits","auto","plan"], default_mode: Some("accept-edits") }`;
  argv per Global Constraints; `classic_argv(id)` = `["agy","--conversation",id]`. Its translator follows Task 0's
  agy recordings: if they match Claude's stream format line for line, delegate to Claude's decoder (permissions
  never arrive); otherwise map agy's own names into the same `Event`s.
- [ ] **Step 1: Failing tests:** `agy_text_fixture_streams_then_finishes`, `agy_tools_fixture_pairs_tools`,
  `agy_argv_has_mode_and_never_skips_permissions` (`--mode accept-edits` by default;
  `--dangerously-skip-permissions` absent even when in `extra_args`), `agy_resume_uses_conversation_flag`.
- [ ] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** → PASS; scene `agent-tab-agy` (AG badge, mode chip).
- [ ] **Step 5: Commit** `M11aa: agy provider`.

### Task 13: Docs and the on-screen checklist

**Files:** Create `docs/04-Development/<next free number> - Milestone 11aa - Agent Tabs.md` (the proposal's Part 1
moves here; the proposal keeps Part 2 for M11ab); modify `40 - Roadmap.md`, `60 - Progress Log.md`, this plan
("At a glance", checkboxes).

- [ ] **Step 1:** Milestone note: goal, what was built, limits, and an **on-screen checklist** for the user, at
  least: ⌃⌘1 opens a shell and Claude side by side; a streamed reply; a tool row opens to a diff; a Write asks and
  Allow / Always / Deny each work; Esc stops; ring + banner when it asks while another workspace is focused; quit
  and relaunch restores the conversation; typing `claude` in the shell is still the classic TUI; an agy tab in each
  mode; drag a failed `npm test` block onto the agent; `fuxx wait --any --for waiting` / `fuxx answer aN yes`.
- [ ] **Step 2:** Roadmap status "🟨 built, on-screen check pending"; Progress Log entry with the test count.
- [ ] **Step 3: Commit** `M11aa: docs`.
