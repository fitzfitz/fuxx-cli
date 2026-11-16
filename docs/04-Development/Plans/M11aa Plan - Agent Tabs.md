---
tags: [development, plan, milestone]
milestone: M11aa
status: safe part built; UI tasks refreshed for M11p/M11ac
updated: 2026-11-16
---

# M11aa Plan — Agent tabs and providers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[Proposal - Chat View and Agent Tabs]] — Part 1 only (Part 2 is M11ab, not planned) |
| **Status** | 🟨 Safe part (Tasks 0, 2, 3, 4, 6) built 2026-11-13 and on `master` (since `5d7c258`). UI tasks started 2026-11-16 ("start M11aa UI next", after M11p merged); Tasks 1, 5, 7–13 **refreshed 2026-11-16** against M11p (cached chrome) and M11ac (`md_preview`). |
| **Order** | After M11z and after M11p (the proposal's "Where it goes"). M11p and M11ac are merged (`master` `8d2b16f`). |
| **Tasks done** | 5 / 14 — Tasks 0, 2, 3, 4, 6 (on `master`; 1661 tests at `8d2b16f`) |
| **▶ Pick up here** | **Task 1**, in worktree `.worktrees/m11aa-ui` (branch `m11aa-ui`). Read "Refresh calls" below and the "Carried forward" blocks in Tasks 7 and 8 first. |
| **Decisions used** | Yours (2026-11-13): chat view for agents (C → C1), fuxx launches the agent, Claude + agy first in their own provider files, one kind per tab, menu presets open a new workspace with a shell + the agent side by side, classic = type the command in a shell, M11aa in M11. Yours (2026-11-16): start the UI tasks; agents stay app children in M11aa, background agents are M11ad (next, design first). AI's calls: the proposal's numbered list, "Plan-time calls" and "Refresh calls" below. |
| **Needs your eyes** | 👀 the on-screen checklist written in Task 13 |

**Goal:** Picking **claude** or **agy** opens a workspace with a shell and an **agent tab** side by side; the agent
tab runs the agent in its structured mode and shows the conversation as a chat (streamed text, tool rows that open
to diffs/output, a todo checklist, an Allow / Always / Deny card), with the tab's status driving rings,
notifications, Activity and the scripting API.

**Architecture:** Pure cores first, GPUI last. `providers/` translates each agent's JSON lines into one fuxx
`Event` language and fuxx `Action`s back (Tasks 2, 12). `agent_conversation.rs` folds events into chat items and a
`Status`; `agent_markdown.rs` parses message text (Task 3; extended by M11ac). `agent_process.rs` runs the agent as a
piped child with a reader thread (Task 4). `TabKind::Agent` / `SavedTab::Agent` add the tab to the layout model and
restore (Task 5). `composer.rs` is a multi-line input (Task 6); `agent_tab.rs` is the view, drawing messages with
M11ac's `md_preview` (Task 7); `workspace_agent.rs` wires it into the workspace, menus, M11p's cached chrome, status,
API and drag-and-drop (Tasks 8–11).

**Tech Stack:** Rust; GPUI at the pinned rev `1d2a4b3f…` — only APIs already used in this repo. `gpui::list` /
`ListState` exist at this rev and are used by the file tab's Preview (`file_tab.rs:2531-2570`); `ListState::splice`
(`list.rs:503`) and `FollowMode::Tail` (`list.rs:113`, `set_follow_mode` `:617`) are in
`~/.cargo/git/checkouts/zed-a70e2ad075855582/1d2a4b3/crates/gpui/src/elements/list.rs`. `futures` unbounded
channels, `std::process` + `std::thread` (the `pty.rs` / `api_server.rs` reader pattern); `serde_json`. Claude Code
≥ 2.1.323 and `agy` ≥ 1.4.3 on PATH. **No new crates.**

**Spec:** [[Proposal - Chat View and Agent Tabs]] (Part 1, §1.1–1.8, "AI's calls", "Where it goes"); for the chrome
rules, [[77 - Milestone 11p - Chrome Caching]].

## Global Constraints

- **No new crates.** `[dependencies]` unchanged. No markdown crate (Task 3 parses a subset itself; M11ac extended it).
- **Work in the git worktree `.worktrees/m11aa-ui` on branch `m11aa-ui`** (off `master` `8d2b16f`). Commit per task
  on the branch; merging and pushing are the user's call. Never commit to `master`.
- **Claude argv, exactly:** `claude -p --input-format stream-json --output-format stream-json --verbose
  --include-partial-messages --permission-prompt-tool stdio` + `--resume <id>` when reopening + any extra preset
  arguments (Task 2's pass-through rule). **Before the first user message** fuxx writes
  `{"type":"control_request","request_id":"fuxx-init","request":{"subtype":"initialize","hooks":null}}`.
  Verified 2026-11-13: **without** the initialize request (`--permission-prompts host` alone) Claude asks no one
  and denies every prompt (`system/permission_denied`).
- **agy argv (Task 0, 2026-11-13):** `agy -p= --input-format stream-json --output-format stream-json --mode <mode>`
  + `--conversation <id>` when reopening. A bare `-p` swallows the next flag as its prompt (exit 2); `-p=` (an
  empty prompt) works. Modes `accept-edits` (default) | `auto` | `plan`. **Never `--dangerously-skip-permissions`.**
- **agy input and output are not Claude's:** each stdin line is `{"event":"user","message":{"content":"<text>"}}`
  (no `role`; a missing `event` is an error). Output is agy's own schema: `{"event":"init",…}`,
  `{"event":"step_update","step_update":{…,"state":"ACTIVE"|"DONE","step_type":"user_input"|"agent_response"|
  "error_message","text_delta"?}}`, `{"event":"result","result":{…,"status":"SUCCESS"|"ERROR"}}`; the conversation id
  is `init.conversation_id`. So agy gets its **own decoder** (Task 12), not Claude's. Its saved recordings are
  credit errors (429), so **re-record the agy fixtures first** (Task 12, Step 0).
- **Agents run on this Mac only**, as children of the app (not the background service): quitting fuxx stops the
  agent, and the tab resumes the conversation with `--resume` on the next launch (yours, 2026-11-16; see Non-goals). Spawned with the login
  shell's `PATH` (`workspace_ssh::login_env()`, `workspace_ssh.rs:186`, background thread only — it can block 3 s),
  `setsid`, piped stdio; closing the tab, the workspace or the app kills the process group.
- **Status mapping (existing `Status`, no new variant):** pending permission → `Waiting`; turn running → `Working`;
  anything said → `Done`; nothing yet → `Idle`. Process exit ≠ 0 or spawn failure → `Done` + an error row.
- **Restore:** only `SavedTab::Agent` restores as a chat. `SavedSession.agent`, `agents_now`,
  `presets::agent_for` and `restore::launch` are **not changed**: a classic `claude` typed in a shell restores
  exactly as today.
- **Scripting ids:** agent tabs are `aN` (`N` from a per-run counter starting at 1), never `sN`.
- **Built-in presets become** `claude`, `agy`, `opencode`, `codex` (⌃⌘1–4). A preset has a provider when its
  **name** is a registered provider's name.
- **M11p's chrome rules bind every UI task** (note 77, `chrome_cache.rs:1-23`, `mark_view.rs:1-19`):
  - **Nothing visible changes inside `Workspace::render`** (a notify made while drawing reaches no observer). An
    agent tab's status, title, question, age or badge reaches the Workspace only through an `AgentTabEvent`
    handled in a `cx.subscribe_in` callback, which updates the Workspace's own mirror (`AgentSlot`, Task 8) and calls
    `cx.notify()` on the Workspace. The cached parts (`ChromePart`) read **only** that mirror and the model — never
    `AgentTab` entities.
  - `ensure_agent_tabs` may run in `render` only as `ensure_file_tabs` does (`file_tab.rs:3942`): it inserts views
    and slots into maps; a new slot starts `Idle`, which no part draws differently (Idle is the least urgent).
  - A moving agent-tab status mark is a `MarkView` (`Workspace::mark_view`, `sidebar.rs:2378`), keyed
    `("agent-tab", aN)`, listed by `marks_on_screen` (`sidebar.rs:1355`) so the ticker (`tick_marks`,
    `workspace.rs:3188`) runs for it and `sync_chrome` (`chrome_cache.rs:177-186`) keeps it among the window's
    entities.
  - Ages: the rows cache's age bucket (`Workspace::rows`, `sidebar.rs:1314`) and `refresh_ages` (`sidebar.rs:1451`)
    include agent slots' `since`. Tab tooltips are built on open (`tab_tip`, `tab_bar.rs:335`).
- **Chrome scenes (M11p style) for every chrome-visible agent-tab source**, in `src/ui_shots/chrome_sources.rs`
  with `Steps::step` (`:176`): change the source by the app's own path (an `AgentTabEvent` from the view, a menu
  handler), frames only via `settle_frames` / `next_frame`, assert the window equals a full redraw and
  (`Seen::Changes`) differs from before; **record RED** by deleting the source's Workspace notify (say which line,
  and the STALE output, in the task report), then restore it.
- **Tests:** inline `#[cfg(test)] mod tests` per module. Fixtures under `src/providers/fixtures/` loaded with
  `include_str!`. Gate for every task:
  `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features ui-shots && cargo fmt --check`.
  **Clippy baseline (measured on `master` `8d2b16f`, 2026-11-16):** `cargo clippy --all-targets 2>&1 | grep -c
  '^warning'` = **30**; with `--features ui-shots` = **32**; no task may raise either. Tests at `8d2b16f`: 1661
  passed, 1 ignored. GUI: `cargo run --features ui-shots -- __ui-shots <filter>` (the filter matches scene names by
  substring, `ui_shots.rs:347`).
- **`#![allow(dead_code)]` (Ruling 2) comes off** each safe-part module (`providers/mod.rs:1`, `providers/claude.rs:1`,
  `agent_conversation.rs:1`, `agent_markdown.rs:1`, `agent_process.rs:1`, `composer.rs:1`) and `presets.rs:27` in the
  task that wires it (Task 7; `provider_for` in Task 8). A warning left after removing it means dead code: delete
  it, or, if a later task in this plan uses it, put an item-level `#[allow(dead_code)] // M11aa Task N` that Task N
  removes. Task 13 checks none is left.
- **Debug builds** keep using `sessions-dev.json` (`restore::state_path`).

## Non-goals (this milestone)

- **Agents that keep running after quit.** Yours, 2026-11-16: in M11aa agent processes stay children of the app;
  quitting fuxx stops the agent and the conversation resumes on the next launch (`--resume`). Agents following the
  "Keep running after quit" setting (the M10a background service hosting agent processes, the tab reattaching on
  relaunch, permission prompts pending while fuxx is closed) is **M11ad — Background agents**, right after M11aa,
  design first, not planned ([[40 - Roadmap]], [[12 - Scope]]).
- The proposal's own non-goals (OpenCode / Codex providers, one conversation per tab, Claude's interactive pickers,
  ACP, exposing conversation text, a mixed timeline) stand.

## Plan-time calls (AI's, made while reading the code; Task 0 copied them into the spec)

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

## Refresh calls (AI's, 2026-11-16, reading `master` `8d2b16f` after M11p and M11ac)

- **R1 — A Workspace mirror of each agent tab.** `Workspace::agents: HashMap<TabId, AgentSlot>` holds the view and
  what the chrome shows (id, provider label/badge, status, since, pop, question, Activity log). Only
  `on_agent_event` (outside drawing) writes the chrome-visible fields, then notifies. Chrome code reads the slot,
  never the entity (note 77: "data that lives in another entity … must reach the Workspace as an event").
- **R2 — Agent marks are keyed by place `"agent-tab"` and the agent id `aN`.** The `marks` map's key is
  `(&'static str, u64)` (`workspace.rs:740`); terminal tabs use `("tab", sN)`, so a separate place keeps the two id
  spaces apart without a new key type. `sync` (`workspace.rs:1646`) retains `"agent-tab"` keys whose id is a live
  slot's.
- **R3 — A row's subject may be an agent tab.** The row shows the most urgent of its terminal rollup and its agent
  tabs (`groups::urgency`, `groups.rs:99`; a tie goes to the terminal). `WorkspaceFacts` / `WorkspaceRow` keep
  `subject: Option<usize>` (the terminal) and gain `agent: Option<TabId>`, set when an agent wins. (The old plan's
  `status::urgency` is not needed: `groups::urgency` exists.)
- **R4 — `TabKind::Agent` carries `cwd`.** Claude keeps conversations per working directory (Task 0: the transcript
  lives under the cwd's realpath), so a restored tab must start where it was started.
- **R5 — `shown_elsewhere` stays terminal-only** (`layout_model.rs:1121`). Two fresh `claude` tabs in one folder are
  equal kinds; deduping them would drop one on restore. Agent tabs are told apart by `TabId`.
- **R6 — Shell-then-agent uses the existing `then` hook.** `open_session_then(launch, at, then: Option<Opened>, …)`
  (`workspace.rs:1376`, `Opened` `:589`) already runs code once the session has opened (the welcome card uses it,
  `welcome.rs:877`). The agent tab is split in there; `restore::Launch` gets no `then_agent` field (the old plan's
  `AgentStart` / `Launch.then_agent` are dropped).
- **R7 — Messages are drawn by `md_preview`.** Task 7 adds `md_preview::column(blocks, base, draw)` (the reader's
  blocks without its page padding) and the agent tab calls it with `md_preview::Draw` / `Colours::of`. Mermaid
  blocks in a chat message show as code (`Draw.mermaid` empty); no Mermaid rendering in the chat in M11aa.
- **R8 — History is read off the UI thread and emitted later.** `Provider::history` reads a file, so it runs on the
  background executor; `AgentTab::new` never emits (its subscriber doesn't exist yet), and no event is emitted from
  `render`.
- **R9 — The status bar keeps following terminals.** With an agent tab focused it shows the workspace's terminal,
  as with a file or browser tab (`follow_terminal`, `right_sidebar.rs:1198`). The Activity panel shows the focused
  agent tab's log (it is not a cached part).
- **R10 — Quit ends agents synchronously in `flush_on_quit`** (`workspace_daemon.rs:465`, registered with
  `on_app_quit` at `workspace.rs:948`), via a new `agent_process::end_all`. `quit_after_flush` only closes the
  window; every quit route reaches `on_app_quit`.
- **R11 — The row label rule.** An unnamed workspace whose named terminal has no user name and no preset is labelled
  with its first agent tab's provider label ("Claude"); otherwise as today (`workspace_label`, `workspace.rs:1859`).
  This also titles the window and the title area.
- **R12 — Shot hooks feed decoded events.** `AgentTab::shot_feed(events)` (ui-shots only) enters the same function
  a decoded process line does, so scenes test the real path from `Event` on; no process runs in a scene.
- **R13 — "Open in classic view" never makes two writers of one conversation** (AI's call, 2026-11-16). Opening it
  first stops the tab's own process (asking first when Working or Waiting), adds the notice row "Open in classic
  view — this tab is paused", and opens `claude --resume <id>` in a terminal tab of the same pane. The tab restarts
  its process (with `--resume`) only when you next send a message from it; if the classic terminal still runs
  `claude` at that moment, the tab warns and keeps your text instead of starting a second writer.

## Review Focus

1. **The agent can't start** (not installed, not on the login PATH, not logged in, exits at once): the tab shows
   one error row with the agent's stderr tail and "Open in classic view", status `Done`, no hang, no panic.
   → Task 4 (`missing_program_is_spawn_failed`, `exit_code_and_stderr_reported`), Task 7
   (`spawn_failure_is_one_error_and_done`, scene `agent-tab-failed`).
2. **Malformed or unexpected output** (a non-JSON line, an unknown `type`, a JSON line split across reads, a line
   over 8 MiB): ignored or reported as one notice, never a crash, the conversation continues.
   → Task 2 (`decode_ignores_unknown_and_garbage`), Task 4 (`lines_survive_split_reads`, `overlong_line_dropped`).
3. **Closing or quitting while the agent works or waits for permission:** confirmation for the tab; after it, no
   `claude` / `agy` process is left running (process group killed, child reaped), even for an agent that ignores
   SIGTERM. → Task 4 (`kill_ends_process_group`), Task 8 (`end_all_ends_a_term_ignoring_agent`, close-confirm
   path).
4. **Restoring a conversation that can't be resumed** (transcript deleted, id unknown): the tab says so in one
   notice row and starts a fresh conversation in the same tab; the session file still loads.
   → Task 2 (`resume_failure_maps_to_an_error`), Task 5 (`unknown_provider_restores_as_error_tab`), Task 7
   (`bad_resume_becomes_one_notice`).
5. **Huge tool output** (a Bash command printing 100k lines): the row shows a one-line summary, the expanded
   output keeps only the tail (call 5), the UI stays responsive.
   → Task 3 (`tool_output_is_capped`), Task 7 (scene `agent-tab-long`).
6. **An agent tab changing where you aren't looking leaves chrome stale** (its tab mark, the sidebar row's ring,
   age or question, the tab title, the badge): every source notifies the Workspace outside drawing.
   → Task 8 (scene `chrome-agent-tabs`), Task 9 (scenes `chrome-agent-status`, `chrome-agent-age`).

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

- [x] **Step 1:** Only when the user says the `master` agent is between tasks: add M11aa (Part 1) and M11ab (Part 2,
  "design first") to Scope and Roadmap after M11p, add a Progress Log entry, and in the proposal flip the warning
  box to "In the plan as M11aa / M11ab", copy in "Plan-time calls", and correct §1.6's "like closing a busy terminal
  today" (call 2) and §1.4's `+` (call 1).
- [x] **Step 2:** Record the clippy warning count on the branch base: `cargo clippy --all-targets 2>&1 | grep -c '^warning'`
  → write it in this plan's "At a glance".
- [x] **Step 3:** Write `scripts/record-agent-fixture.py <agent> <scenario> <workdir> <out>`: starts the argv from
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
- [x] **Step 4:** Run every scenario for Claude, then for agy (skip agy's permission scenarios). Scrub absolute
  temp paths to `/tmp/fx` and the user's home to `~` in the files.
- [x] **Step 5: Go/no-go.** Write into the proposal's "Claude Code headless mode" section what each recording
  shows, answering: does `deny` with a message reach the model; does `updatedPermissions` make the next identical
  Write skip the prompt; does `interrupt` stop the turn and what arrives after it; can a second user message be
  sent while a turn runs (and what `queued_turn_count` says); does `--resume` replay history on stdout, and if not,
  where the transcript file is; does `/compact` run; what `bad-resume` prints; where context-window size appears
  (`result.modelUsage`?) for the `context N%` line; agy's event names for text, tools, conversation id, turn end.
  **If allow or deny doesn't work, stop and report to the user** (the proposal's gate). Otherwise continue.
- [x] **Step 6: Commit** — `git add docs scripts/record-agent-fixture.py src/providers/fixtures && git commit -m "M11aa: plan, scope and agent fixtures"`

### Task 1: Presets know about providers

**Files:** Modify `src/presets.rs` (`builtins` `:36`, module doc `:1` "the built-in three", test
`builtins_are_claude_opencode_codex` `:134`). Test: inline.

**Interfaces:**
- Produces: `presets::builtins()` → `claude`, `agy`, `opencode`, `codex` (each `command` = its name);
  `pub fn extra_args(preset: &Preset) -> Vec<String>` — the preset command's words after the first, split on
  whitespace. (`provider_for` exists since Task 2, `presets.rs:28`; its `#[allow(dead_code)]` at `:27` comes off in
  Task 8.)
- Unchanged: `procs::AGENTS` (`procs.rs:61`) — a classic `agy` typed in a shell is detected as today.

- [ ] **Step 1: Failing tests** (replace `builtins_are_claude_opencode_codex`)

```rust
#[test] fn builtins_are_claude_agy_opencode_codex() {
    assert_eq!(names(&builtins()), ["claude", "agy", "opencode", "codex"]);
}
#[test] fn extra_args_are_words_after_the_program() {
    let p = Preset { name: "claude".into(), command: "claude  --model sonnet".into() };
    assert_eq!(extra_args(&p), ["--model", "sonnet"]);
    assert!(extra_args(&Preset { name: "x".into(), command: "x".into() }).is_empty());
}
```

- [ ] **Step 2:** `cargo test --bin fuxx presets` → FAIL. **Step 3:** implement. **Step 4:** → PASS; then fix the
  preset tests that count built-ins: `user_presets_add_replace_and_remove` (`:147`) →
  `["claude", "agy", "opencode", "aider"]` and `p[3].command == "aider --model sonnet"`;
  `bad_lines_and_overflow_are_skipped_with_warnings` (`:165`) → `p[8].name == "p4"` (4 built-ins + 7; `warnings.len()`
  stays 3: one line names every skipped preset). `welcome.rs:1021` passes `3` itself and stays. Run
  `cargo run --features ui-shots -- __ui-shots welcome` and look at the welcome card with four preset rows (it must
  fit; if not, say so in the report). Full gate.
- [ ] **Step 5: Commit** `M11aa: agy built-in preset`.

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

- [x] **Step 1: Failing tests** (in `claude.rs`; `decode_all(p, text)` is a test helper feeding each line):

```rust
#[test] fn text_fixture_streams_then_finishes() {
    let ev = decode_all(&CLAUDE, include_str!("fixtures/claude-text.jsonl"));
    assert!(matches!(ev.first(), Some(Event::Started { .. })));
    assert!(ev.iter().any(|e| matches!(e, Event::TextDelta { .. })));
    assert!(matches!(ev.last(), Some(Event::TurnDone { ok: true, .. })));
}
#[test] fn tools_fixture_pairs_every_start_with_a_finish() { /* each ToolStarted id has one ToolFinished; Edit has Diff */ }
// TodoWrite is not offered in this Claude build, so no fixture shows it (Task 0, Ruling 8): map it when present and
// add `todo_write_maps_to_a_todo_event`, driven by one hand-written line marked `synthetic` in its comment.
// `history()`: `--resume` does not replay on stdout (Task 0); the transcript is
// `~/.claude/projects/<realpath cwd with / and . turned into ->/<id>.jsonl` (plan-time call 7). `history()` may stay
// at the default here; Task 7 needs the real one (it reads that file).
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

- [x] **Step 2:** `cargo test --bin fuxx providers` → FAIL. **Step 3:** implement. **Step 4:** → PASS.
- [x] **Step 5: Commit** `M11aa: providers and the Claude translator`.

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

- [x] **Step 1: Failing tests**

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

- [x] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** → PASS. **Step 5: Commit** `M11aa: conversation model and markdown`.

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

- [x] **Step 1: Failing tests** (each collects events with `futures::executor::block_on` + a 5 s timeout)

```rust
#[test] fn echoes_lines_back() { /* argv ["/bin/sh","-c","cat"]; send("a"); send("b") → Line("a"), Line("b") */ }
#[test] fn lines_survive_split_reads() { /* sh -c 'printf "{\"x\":"; sleep 0.2; printf "1}\n"' → one Line("{\"x\":1}") */ }
#[test] fn overlong_line_dropped() { /* a 9 MiB line then "ok" → the overlong marker, then Line("ok") */ }
#[test] fn exit_code_and_stderr_reported() { /* sh -c 'echo nope >&2; exit 3' → Exited{code:Some(3), stderr_tail contains "nope"} */ }
#[test] fn missing_program_is_spawn_failed() { /* argv ["/nonexistent/agent"] → SpawnFailed(_) */ }
#[test] fn kill_ends_process_group() { /* sh -c 'sleep 100 & sleep 100'; kill(); after 3 s `kill(-pgid, 0)` fails with ESRCH */ }
```

- [x] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** → PASS. **Step 5: Commit** `M11aa: agent process runner`.

### Task 5: `TabKind::Agent` in the layout model and restore

**Files:** Modify `src/layout_model.rs` (`TabKind` `:29`, `SavedTab` `:194`, `to_saved` `:822` (File arm `:843`),
`restored_pane` `:1053`, `saved_focus` `:1196`, `SavedFocus` `:1212`; `shown_elsewhere` `:1121` **unchanged**, R5),
`src/restore.rs` (`:376`, `:390`), and every exhaustive match on `TabKind` / `SavedTab` (re-grep
`rg -n "TabKind::File|SavedTab::File" src` first; on `8d2b16f`): `adopt.rs:224-234` (`SavedFocus`),
`adopt.rs:427,439,964,1090,1102`, `palette.rs:1017,1043`, `changes_panel.rs:1267`, `tab_bar.rs:95,135,138,786,1731`,
`file_tab.rs:3951`, `sidebar.rs:1404`, `workspace.rs:1732,1749,1876`, `workspace_browser.rs:382`, and the
`ui_shots.rs` matches (build with `--features ui-shots` too). Test: inline in `layout_model.rs`.

**Interfaces:**
- Produces:

```rust
pub enum TabKind { Terminal(u64), Browser { .. }, File { .. },
    Agent { provider: String, cwd: PathBuf, conversation: Option<String>, mode: Option<String>, title: Option<String> } }
pub enum SavedTab { .., Agent { provider: String, cwd: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")] conversation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] title: Option<String> } }   // JSON kind "agent"
pub enum SavedFocus { Terminal(usize), Browser, File, Agent }
impl Model { pub fn agent_tabs(&self, ws: WorkspaceId) -> Vec<TabId>; }   // reading order, as `terminals`
```
`SAVED_VERSION` stays 2 (as File did; an older fuxx can't read a file with an agent tab — same accepted limit as
File tabs). Agent tabs move between panes like File tabs. New match arms elsewhere do the minimum to compile and act
like a file tab (no session id; `tab_bar::icon` → `TabIcon::File`, title = the provider name); Tasks 8–10 fill them
in. `reconcile` (`:547`) and `terminals` (`:254`) ignore agent tabs (they are not terminals).

- [ ] **Step 1: Failing tests**

```rust
#[test] fn agent_tab_round_trips_through_saved() {
    // new_workspace with Terminal(1), split with Agent{provider:"claude",cwd:"/w/app",conversation:Some("c1"),mode:None,title:Some("fix")}
    // to_saved → JSON contains "\"kind\":\"agent\"", "\"cwd\":\"/w/app\"", "\"conversation\":\"c1\"" and no "mode";
    // from_saved → the same kinds back, in the same panes
}
#[test] fn two_fresh_agent_tabs_in_one_folder_both_restore() { /* two Agent{provider:"claude",cwd:"/w",None..} → both come back (R5) */ }
#[test] fn old_files_without_agent_tabs_still_load() { /* a v2 JSON with only terminal/file tabs decodes unchanged */ }
#[test] fn unknown_provider_restores_as_error_tab() { /* SavedTab::Agent{provider:"gone",..} restores as TabKind::Agent{provider:"gone",..}; Task 7's view shows the error */ }
#[test] fn agent_tabs_lists_only_agents() { /* workspace with Terminal + Agent + File → agent_tabs == [that tab]; terminals == [the terminal] */ }
#[test] fn a_focused_agent_tab_is_saved_focus_agent() { /* saved_focus → Some(SavedFocus::Agent) */ }
```

- [ ] **Step 2:** → FAIL. **Step 3:** implement; `cargo build` and `cargo build --features ui-shots` until every
  match compiles. **Step 4:** → PASS (whole suite). Full gate.
- [ ] **Step 5: Commit** `M11aa: agent tabs in the layout model`.

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

- [x] **Step 1: Failing tests** — `EditBuffer`: `newline_then_up_keeps_column`, `backspace_joins_lines`,
  `rows_counts_newlines` (`"a\nb\nc"` → 3), `insert_replaces_selection`.
- [x] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** → PASS. **Step 5: Commit** `M11aa: multi-line composer`.

### Task 7: The agent tab view

**Files:** Create `src/agent_tab.rs`; modify `src/agent_conversation.rs` (`past`, `failed_to_start`),
`src/providers/mod.rs` (`Past`, `Provider::history`), `src/providers/claude.rs` (`history`, `describe_suggestion`
`:201`), `src/agent_process.rs` (nothing new; its allow comes off), `src/composer.rs` (soft-wrap, wheel, deferred
minors), `src/md_preview.rs` (`column`), `src/main.rs` (`mod agent_tab;`), `src/ui_shots.rs` (scenes; `SCENES`
`:215-260`), `scripts/record-agent-fixture.py` (transcript copy). Remove `#![allow(dead_code)]` from
`providers/mod.rs`, `providers/claude.rs`, `agent_conversation.rs`, `agent_markdown.rs`, `agent_process.rs`,
`composer.rs` (Global Constraints).

**Interfaces:**
- Consumes: Tasks 2, 3, 4, 6; `md_preview::{Draw, Colours, link_target, LinkTarget}` (`md_preview.rs:262,209,62`).
- Produces:

```rust
// agent_tab.rs
pub enum AgentTabEvent {
    Status { status: Status, live: bool },   // live: false while replaying history (no banner, no pop)
    Question(Option<String>),               // "Claude wants to run: <what>" while Waiting, else None
    Title(String), Conversation(String), Close, OpenClassic(Vec<String>),
}
pub struct AgentTab;   // EventEmitter<AgentTabEvent>, Focusable, Render
impl AgentTab {
    pub fn new(provider: Option<&'static dyn Provider>, provider_name: String, start: Start,
               window: &mut Window, cx: &mut Context<Self>) -> Self;   // None provider → error tab; spawns, never emits
    pub fn send(&mut self, text: String, cx: &mut Context<Self>);
    pub fn attach(&mut self, attachment: String, cx: &mut Context<Self>);
    pub fn answer(&mut self, answer: Answer, cx: &mut Context<Self>) -> Result<(), &'static str>; // Err "not waiting"
    pub fn stop(&mut self, cx: &mut Context<Self>);
    pub fn status(&self) -> Status;
    pub fn question(&self) -> Option<String>;
    pub fn mode(&self) -> Option<&str>;
    #[cfg(feature = "ui-shots")] pub fn for_shots(provider: &'static dyn Provider, start: Start, window: &mut Window, cx: &mut Context<Self>) -> Self; // no process
    #[cfg(feature = "ui-shots")] pub fn shot_feed(&mut self, events: Vec<Event>, cx: &mut Context<Self>);   // R12
}
pub const STOP_FORCE_AFTER: Duration = Duration::from_secs(10);
pub const END_WAIT: Duration = Duration::from_secs(1);   // after Ended, wait this long for Exited's stderr tail
#[derive(Default)] pub(crate) struct ProcessEnd;   // reports the process's end exactly once
impl ProcessEnd { pub fn ended(&mut self, code: Option<i32>) -> bool;           // true: start the END_WAIT timer
                  pub fn exited(&mut self, code: Option<i32>, tail: String) -> Option<(Option<i32>, String)>;
                  pub fn waited(&mut self) -> Option<(Option<i32>, String)>; }  // the timer fired first: (code, "")
// providers/mod.rs
pub enum Past { You(String), Event(Event) }
trait Provider { fn history(&self, start: &Start) -> Vec<Past> { Vec::new() } /* signature changes from Vec<Event> */ }
// agent_conversation.rs
impl Conversation {
    pub fn past(&mut self, past: Vec<Past>);          // You → Item::You without counting a send; then settle(): Done if anything
    pub fn failed_to_start(&mut self, message: &str);  // one Error row, ended → Done
}
// md_preview.rs
pub fn column(blocks: &[Md], base: usize, draw: &Draw) -> AnyElement;   // blocks_of (`:336`) in a column, no page padding; link ids from `base << 20`
```

**Layout**, from the mockup `c1-agent-tab.html` and spec §1.1–1.2, colours from `theme::UiColors` only:
- Top line: status word + `context N%` when known (`text::META`, `text_dim`).
- Items in a `gpui::list` (`ListState::new(n, ListAlignment::Top, px(200.))`, `FollowMode::Tail`); an item added →
  `splice(n..n, 1)`; an item changed (delta, tool finished, answer) → `splice(i..i+1, 1)`. `You` right-aligned on
  `card` with `card_border`, attachments as chips; `Agent` = badge avatar + `md_preview::column(&parse(text), i, &draw)`
  (parse cached per item until its text changes; `Draw.mermaid` empty, R7; a link: `LinkTarget::Web` → `cx.open_url`,
  anything else nothing); `Tool` row on `panel`, tool name in `accent`, summary, result at right, spinner + elapsed
  seconds while running; click toggles detail (Diff through `diff_view::parse_diff` (`diff_view.rs:67`) + the file
  tab's row colours; Command/output in the terminal font, with the dropped-lines row); sub-agent tools nested under
  their parent, collapsed by default; `Todo` as ✓ / ◐ / ○ rows; `Permission` card with `ring_waiting` border, `what`
  and `detail`, buttons **Allow ⏎**, **⟨always⟩** (only when Some), **Deny**; answered cards collapse to one line
  ("Allowed", "Denied: reason", "Expired"); `Notice` in `text_dim`; `Error` in `block_failed` with **Open in classic
  view** (emits `OpenClassic(provider.classic_argv(id))`) when a conversation id exists, and **Close** on an error tab.
- Composer (Task 6's `Composer`): mode chip (agy, before the first message, call 6), model chip, cost chip, hints
  "/ commands · @ files"; while Working the right button is **■ Stop (esc)**; with a pending permission the
  placeholder is "Deny and say why… (optional)", Enter on an empty composer = Allow, Enter with text = Deny with that
  reason. Messages sent mid-turn go to the process at once (Claude queues them; `Conversation` counts sends).
- `@`: a popup over the composer listing `palette_files::list_files(&start.cwd)` (`palette_files.rs:234`, run on the
  background executor once per tab, then kept) ranked by `palette_files::rank_files` (`:374`, limit 8); Enter
  inserts the relative path.
- Empty state: "Ask ⟨Label⟩ to do something in ⟨~/short/cwd⟩" (`sidebar::home_relative`), composer focused.
- The view is drawn cached by the Workspace (Task 8, like the file tab at `workspace.rs:2933-2937`): every change it
  shows ends in `cx.notify()` on the AgentTab.

**Wiring inside the tab:**
- `new`: no process for `None` provider (one `Error("No agent called ⟨name⟩")` row). Otherwise a `cx.spawn` task
  that, on the background executor, reads `login_env()`'s `PATH` and `provider.history(&start)`, then (back on the
  entity) applies `Conversation::past`, writes `translator.hello()`, spawns `agent_process::spawn(argv, cwd, path,
  tx)` and loops on `rx`, each `ProcessEvent` → `on_process` (R8: nothing is emitted from `new` or `render`).
- `on_process`: `Line` → `translator.decode` → `on_events`; `Ended` / `Exited` / the `END_WAIT` timer through
  `ProcessEnd` → `Conversation::process_ended` **once** (it isn't idempotent); `SpawnFailed(m)` → `failed_to_start`.
- `on_events(events)` (also `shot_feed`'s entry): apply each, splice the list, then emit what changed since the last
  call, in this order — `Question` when `question()` changed, `Status` when `conversation.status()` changed (so a
  Waiting banner has its question), `Title` on the first
  `You` (`conversation.title()`), `Conversation` on the first `Started` — and `cx.notify()`.
- **Bad resume:** `start.resume` set and the process ends before any `Started` → drop the rows that run produced, add
  `Notice("Couldn't resume that conversation; this is a new one")`, respawn with `resume: None`.
- **Stop:** `Action::Interrupt`; the button reads "Stopping…" until `TurnDone`; after `STOP_FORCE_AFTER` it offers
  **Force stop**: kill, then respawn with `--resume <id>`.
- `history()` for Claude: the transcript at `~/.claude/projects/<realpath(cwd) with / and . → ->/<id>.jsonl`;
  each line through a fresh translator's `decode` (`assistant` / `user` tool_result lines are the stream's shapes),
  plus a `user` line whose content is a string → `Past::You`; a missing or unreadable file → `[]`, and the view adds
  `Notice("Conversation restored")` when `start.resume` is set and history is empty (call 7).
- `describe_suggestion` (`claude.rs:201`) returns the button's whole label: `setMode acceptEdits` → **"Allow all
  edits this session"**; `addRules` → `"Always allow Bash(ls)"` (Ruling 10); the view shows `always` verbatim.

**Carried forward into this task (from the safe-part run, 2026-11-13):**
- **History:** Claude's `history()` reads the transcript at `~/.claude/projects/<realpath of cwd with / and . turned into ->/<id>.jsonl` (`--resume` does not replay on stdout).
- **Stop is best-effort:** "Stopping…" until `TurnDone`, **Force stop** after 10 s. A stop arrives as `Notice("Stopped")` then a failed `TurnDone` (no red row).
- **"Always allow" for Write/Edit** means "Allow all edits this session" (`setMode acceptEdits`); the label must say so.
- **Process exit:** use `ProcessEvent::Ended` for "the process is gone"; a straggler holding stdout can delay `Exited`; `process_ended` must be called once (on the end, not again on `Exited`).
- **Composer:** soft-wraps long lines; ↑/↓ move by visual row; shape only the on-screen rows; wheel-scroll it. Deferred Task 6 minors: ⌘←/⌘→ with a selection go to line start/end, not the selection edge (`composer.rs:228-236`, `:250-259`); `point_for` checks the row is below the visible window (`:571-582`); add "Composer" key-context tests.
- **Expired permissions:** cards with `Answer::Expired` show as expired, not as open.
- **Bad resume** and **spawn failure**: as in "Wiring" above.

- [ ] **Step 0: Record a transcript fixture.** Extend `scripts/record-agent-fixture.py` so the `text` scenario also
  copies `~/.claude/projects/<mangled realpath of workdir>/<session_id>.jsonl` to
  `src/providers/fixtures/claude-transcript.jsonl`, through the same always-on scrub (Rulings 4, 11). Run it once;
  read the file; if its lines are not the stream's `assistant` / `user` shapes, write what they are in the task
  report and map them in `history` accordingly.
- [ ] **Step 1: Failing tests**

```rust
// agent_conversation.rs
#[test] fn past_turns_read_done_not_working() {
    let mut c = Conversation::new();
    c.past(vec![Past::You("hi".into()), Past::Event(Event::MessageDone { message: "m".into(), text: "Hello".into() })]);
    assert_eq!(c.status(), Status::Done); assert_eq!(c.items().len(), 2);
}
#[test] fn spawn_failure_is_one_error_and_done() {
    let mut c = Conversation::new(); c.failed_to_start("No such file");
    assert_eq!(c.status(), Status::Done);
    assert!(matches!(c.items(), [Item::Error(e)] if e.contains("No such file")));
}
// providers/claude.rs
#[test] fn history_reads_the_transcript() { /* transcript_events(include_str!("fixtures/claude-transcript.jsonl")) → [You(..), .., Event(MessageDone{..})] */ }
#[test] fn transcript_path_mangles_the_cwd() {   // pure: the caller passes the realpath
    assert_eq!(transcript_path(Path::new("/Users/me"), Path::new("/Users/me/k.p/app"), "id1"),
               PathBuf::from("/Users/me/.claude/projects/-Users-me-k-p-app/id1.jsonl"));
}
// (update `permission_request_decodes_to_a_card`, claude.rs:703: always == "Allow all edits this session")
// agent_tab.rs (pure)
#[test] fn process_end_is_reported_once() {
    let mut e = ProcessEnd::default();
    assert!(e.ended(Some(1)));
    assert_eq!(e.exited(Some(1), "nope".into()), Some((Some(1), "nope".into())));
    assert_eq!(e.waited(), None);
}
#[test] fn bad_resume_becomes_one_notice() { /* pure helper `resume_failed(started: bool, resume: bool) -> bool`: (false,true) → true; (true,true) / (false,false) → false */ }
// md_preview.rs
#[test] fn column_link_ids_start_at_base() { /* Draw.links after column(blocks, 3, &draw) starts at 3 << 20 */ }
// composer.rs: soft-wrap and key-context tests as carried forward
#[test] fn up_and_down_move_by_visual_row() { /* EditBuffer with a wrap width: a line wrapped in 3 → down twice stays in that line */ }
```
  Scenes (no process; `AgentTab::for_shots` + `shot_feed` with fixture events), each in Mocha and Latte:
  `agent-tab-empty`, `agent-tab-working` (tools fixture mid-way), `agent-tab-permission`, `agent-tab-done`,
  `agent-tab-failed` (`process_ended(Some(1), "not logged in")`), `agent-tab-long` (300 items plus a 100k-line
  tool output, scrolled to the end; asserts fewer than 40 item rows were laid out), `agent-tab-markdown` (a reply
  with a heading, list, table, quote and code block, drawn by `md_preview::column`), and
  `agent-tab-cached-current`: the view embedded `.cached(size_full)` as Task 8 will; `shot_feed` a delta, a tool
  finish and a permission, each followed by `settle_frames` only → the frame equals a full redraw and changed (RED:
  remove `on_events`' `cx.notify()`). Run `cargo run --features ui-shots -- __ui-shots agent-tab` → fails.
- [ ] **Step 2:** Implement. **Step 3:** tests and scenes pass; compare each PNG with the mockup; check text
  contrast ≥ `TEXT_MIN` with `ui_tokens::contrast` for `text_dim` on `panel` and on `card`.
- [ ] **Step 4:** Full gate (both clippy counts at or under 30 / 32 with the six allows gone).
- [ ] **Step 5: Commit** `M11aa: agent tab view`.

### Task 8: Agent tabs in the workspace

**Files:** Create `src/workspace_agent.rs` (pattern: `workspace_browser.rs`, `file_tab.rs:3942-4040`); modify
`src/workspace.rs` (fields beside `file_tabs` `:672`; `Render` `:3263-3264` (call `ensure_agent_tabs`);
`render_panes`' pane view `:2929-2938`; `focus_active` `:2882`; `sync` `:1625` (slots retain, marks retain
`:1646`); `launch_preset` `:2025`; `workspace_label` `:1859` (R11); `Renaming` `:573`, `start_tab_rename` `:2651`,
`on_name_event` `:2692-2713`), `src/welcome.rs` (`run_welcome_pick` `:877`), `src/palette.rs` (`Key::Preset`
`:1166`), `src/tab_bar.rs` (`TabIcon` `:54`, `icon` `:92`, `bar_tabs` `:117`, `TabChip` `:280`, `pane_bar_tabs`
`:344`, `render_tab` `:521-550`, `close_tab` `:778`), `src/tab_menu.rs` (`busy` `:276`, `close_workspace` `:236`,
`end_tabs` `:312`), `src/pane_actions.rs` (`PaneAction` `:32`, `ALL` `:41`, `run_pane_action` `:104`, menu `:139`),
`src/pane_tab_menu.rs` (`PaneTabAction` `:39`, `enabled` `:67`), `src/palette_items.rs` (pane-action rows, test
`:1346`), `src/sidebar.rs` (preset menu `:2750-2770`), `src/workspace_daemon.rs` (`flush_on_quit` `:465`),
`src/agent_process.rs` (`end_all`), `src/agent_tab.rs` (`process`, `end`, the classic pause),
`src/agent_conversation.rs` (`paused`), `src/presets.rs` (remove `:27`'s allow), `src/ui_shots/chrome_sources.rs`,
`src/ui_shots.rs`.

**Interfaces:**
- Consumes: `AgentTab`, `AgentTabEvent` (Task 7), `TabKind::Agent`, `Model::agent_tabs` (Task 5),
  `presets::{provider_for, extra_args}` (Tasks 1–2), `open_session_then` + `Opened` (`workspace.rs:1376`, `:589`; R6).
- Produces:

```rust
// workspace_agent.rs
pub(crate) struct AgentSlot {
    pub view: Entity<AgentTab>,
    pub id: u64,                                   // aN, from Workspace::next_agent_id (starts at 1)
    pub label: SharedString,                       // provider label; unknown provider: its name
    pub badge: &'static str, pub badge_rgb: u32,   // unknown provider: "?" and 0x6b6b6b
    pub status: Status, pub since: Instant,        // Idle / now until Task 9 writes them
    pub pop: Option<(u64, Instant)>, pub question: Option<String>,
    pub activity: ActivityLog,
}
pub enum LaunchPlan { Classic, ShellThenAgent(&'static str) }    // the provider's name
pub fn launch_plan_for(preset: &Preset) -> LaunchPlan;            // ShellThenAgent iff provider_for(preset) is Some
pub fn agent_row_label<'a>(first_agent: Option<&'a str>, terminal_named: bool) -> Option<&'a str>; // R11
impl Workspace {
    pub(crate) fn ensure_agent_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>);  // render-safe (Global Constraints)
    pub(crate) fn agent_start(&self, kind: &TabKind, cx: &App) -> Option<Start>;  // cwd, resume = conversation, mode, extra_args from today's preset of that name
    pub(crate) fn pane_agent_tab(&self, pane: PaneId) -> Option<Entity<AgentTab>>;
    pub(crate) fn focused_agent_tab(&self) -> Option<(TabId, Entity<AgentTab>)>;
    pub(crate) fn new_agent_tab(&mut self, pane: PaneId, provider: &'static str, window: &mut Window, cx: &mut Context<Self>);
    pub(crate) fn agent_beside(&mut self, terminal: u64, provider: &'static str, window: &mut Window, cx: &mut Context<Self>);
    pub(crate) fn close_agent_tab(&mut self, tab: TabId, ask: bool, window: &mut Window, cx: &mut Context<Self>);
    fn on_agent_event(&mut self, tab: TabId, event: &AgentTabEvent, window: &mut Window, cx: &mut Context<Self>);
    pub(crate) fn workspace_is_ssh(&self, ws: WorkspaceId) -> bool;   // any terminal with `ssh`
}
// workspace.rs: pub(crate) agents: HashMap<TabId, AgentSlot>, next_agent_id: u64; Renaming::Agent(TabId)
// tab_bar.rs
pub enum TabIcon { Terminal, Globe, File, Mark(Status), Agent { badge: &'static str, rgb: u32 } }
pub struct AgentFacts { pub label: SharedString, pub badge: &'static str, pub rgb: u32, pub status: Status, pub age: Duration }
pub fn bar_tabs(pane: &Pane, facts: impl Fn(u64) -> Option<TerminalFacts>, page: impl Fn(TabId) -> Option<SharedString>,
                agent: impl Fn(TabId) -> Option<AgentFacts>) -> Vec<BarTab>;
// pane_actions.rs: PaneAction::NewAgent(&'static str) (label "New ⟨Label⟩ tab"); `ALL` becomes
pub fn items(local: bool) -> Vec<PaneAction>;   // NewTerminal, NewBrowser, one NewAgent per providers::all() when local, SplitRight, SplitDown
// pane_tab_menu.rs: PaneTabAction::OpenClassic (enabled: an agent tab with a conversation); Rename enabled for agent tabs
// agent_process.rs
pub fn end_all(processes: &[Arc<AgentProcess>], grace: Duration);   // SIGTERM every group, wait ≤ grace for each reap, SIGKILL the rest; blocking
// agent_tab.rs: pub fn process(&self) -> Option<Arc<AgentProcess>>; pub fn end(&mut self)   // kill, no respawn
//   R13: pub fn pause_for_classic(&mut self, cx);   // end the process (no exit row), Conversation::paused()
//        pub fn resume_after_classic(&mut self, text: String, classic_running: bool, cx);   // per after_classic
//   AgentTabEvent gains ResumeAsked(String): `send` on a paused tab emits it instead of sending
// agent_conversation.rs
pub const PAUSED_NOTICE: &str = "Open in classic view — this tab is paused";
impl Conversation { pub fn paused(&mut self); }   // settle(), one Notice(PAUSED_NOTICE), reads Done; no exit row
// workspace_agent.rs (R13)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterClassic { Send, Respawn, Warn }
/// What sending from an agent tab does: not paused → Send; paused and the classic terminal still runs the agent →
/// Warn (a Notice "⟨Label⟩ is still running this conversation in ⟨terminal⟩ — quit it there first", the text stays
/// in the composer); paused and it doesn't → Respawn with `--resume`, then send.
pub fn after_classic(paused: bool, classic_running: bool) -> AfterClassic;
impl Workspace { pub(crate) fn open_classic(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>); }
// Workspace: pub(crate) classic_of: HashMap<TabId, u64>   // agent tab → the sN its classic view opened
```
- **Launching:** `launch_preset(n)`, the welcome card's `Choice::Preset` (`run_welcome_pick`) and ⌘K's
  `Key::Preset` ask `launch_plan_for`. `ShellThenAgent(name)`: open the shell with `preset: None` and
  `run_preset: false` (restore never re-types it) through `open_session_then(launch, At::New, Some(then))`, where
  `then` (after the welcome card's own close-after, when both apply) calls `agent_beside(id, name)` if the session
  opened. `agent_beside`: `model.split(the terminal's pane, Axis::Row, TabKind::Agent { provider, cwd, conversation:
  None, mode: info.default_mode, title: None })` with `cwd` = the session's `project`, else `places.home`; then
  `sync`, `save`, `cx.notify()` — the view itself is made by `ensure_agent_tabs` on the next frame and takes the
  keyboard there (as `ensure_file_tabs`). `Classic`: unchanged.
- **The view in a pane:** `render_panes` embeds `pane_agent_tab(id)` with `.cached(StyleRefinement::default().size_full())`
  like the file tab; `focus_active` focuses `focused_agent_tab()`; `sync` drops slots whose tab is gone (calling
  `AgentTab::end` first) and retains `"agent-tab"` marks only for live slot ids.
- **Events (`on_agent_event`, subscribed with `cx.subscribe_in` when the slot is made):** `Title(t)` → when
  `TabKind::Agent.title` is None, `set_kind` with `title: Some(t)`, `save()`, `cx.notify()`; `Conversation(id)` →
  `set_kind` with `conversation: Some(id)`, `save()`; `Close` → `close_agent_tab(tab, false)`; `OpenClassic(_)` →
  `open_classic(tab)`; `ResumeAsked(text)` → `AgentTab::resume_after_classic(text, classic_running)` where
  `classic_running` = `classic_of[tab]` is a live session whose `programs.state` is `Running(<provider name>)`
  (looked up every 3 s, so a just-quit `claude` may still read running for up to 3 s: the warning says to try again);
  `Status` / `Question` → Task 9.
- **Open in classic view (R13, AI's call):** `open_classic(tab)` — Working or Waiting → a native sheet ("Open
  ⟨title⟩ in classic view?", "⟨Label⟩'s turn stops.", Open / Cancel); then `AgentTab::pause_for_classic`, then
  `open_session_then(Launch { saved: SavedSession { project: cwd, preset: Some(Preset { name: provider, command:
  provider.classic_argv(id).join(" ") }), .. }, run_preset: true }, At::Tab(pane), Some(then))` with `then`
  recording `classic_of.insert(tab, sN)`. The paused tab's status reads Done (no banner: `live: false`).
- **Tab bar:** `TabIcon::Agent` is a 16×16 rounded badge (`badge_rgb` fill, its two letters in white at `text::META`)
  in `render_tab` and `TabChip`; title = `TabKind::Agent.title` or `slot.label`; `tip` = `tab_bar::tip(title, None,
  status, age)` through `tab_tip` (built on open).
- **Closing:** `close_tab`'s agent arm → `close_agent_tab(tab, true)`: Working or Waiting → a native sheet
  ("Close ⟨title⟩?", "⟨Label⟩ is still working." / "… is waiting for you.", Close / Cancel), else at once; closing =
  `AgentTab::end`, `model.close_tab`, `sync`, `save`, then as `close_browser_tab` (`all_gone` → quit, else
  `activate`). `tab_menu::busy` counts agent tabs Working or Waiting; `close_workspace` asks when
  `asks_to_close(terminals)` **or** an agent tab in it is busy; `end_tabs` (already asked) closes agent tabs with
  `ask: false`.
- **Menus:** `···` lists `PaneAction::items(!workspace_is_ssh(ws))`; ⌘K lists the same pane actions; the sidebar's
  preset menu draws the provider badge before a preset's name when `provider_for` is Some; Rename Tab… on an agent
  tab opens the rename box over its title (`Renaming::Agent(tab)`; Confirm → `set_kind` title, empty → None,
  `save`, `cx.notify()`); **Open in classic view** calls `open_classic(tab)`.
- **Row label (R11):** `workspace_label` — after the workspace's own name — returns `agent_row_label(first agent
  tab's slot label, named terminal has a user name or a preset)` when Some.
- **Quit (R10; yours 2026-11-16: agents stop with the app, M11ad is the follow-up):** `flush_on_quit` also
  collects `slot.view.read(cx).process()` for every slot and, in its
  background task, runs `agent_process::end_all(&processes, TERM_GRACE)` before the pumps' flush (`TERM_GRACE`,
  `agent_process.rs:22`, becomes `pub`).

**Carried forward into this task (from the safe-part run, 2026-11-13):**
- **Quit must kill synchronously:** an agent that ignores SIGTERM must still die before the app exits; don't rely on the killer thread or `Drop` (→ `end_all` in `flush_on_quit`).
- **Process exit vs `Exited`:** closing a tab whose agent exited on its own while a straggler holds stdout is a no-op for `kill()` (the reaped pgid is not signalled); treat `Ended` as "done" and don't wait for `Exited` (`end_all` waits on the reap, not on `Exited`).

- [ ] **Step 1: Failing tests**

```rust
// workspace_agent.rs
#[test] fn launch_plan_follows_the_provider() {
    let p = |n: &str, c: &str| Preset { name: n.into(), command: c.into() };
    assert_eq!(launch_plan_for(&p("claude", "claude")), LaunchPlan::ShellThenAgent("claude"));
    assert_eq!(launch_plan_for(&p("claude", "claude --model sonnet")), LaunchPlan::ShellThenAgent("claude"));
    assert_eq!(launch_plan_for(&p("codex", "codex")), LaunchPlan::Classic);
}
#[test] fn an_unnamed_shell_row_takes_the_agent_label() {
    assert_eq!(agent_row_label(Some("Claude"), false), Some("Claude"));
    assert_eq!(agent_row_label(Some("Claude"), true), None);
    assert_eq!(agent_row_label(None, false), None);
}
// pane_actions.rs
#[test] fn pane_menu_offers_agents_only_locally() {
    assert!(PaneAction::items(true).contains(&PaneAction::NewAgent("claude")));
    assert!(!PaneAction::items(false).iter().any(|a| matches!(a, PaneAction::NewAgent(_))));
}
#[test] fn sending_after_classic_view_never_makes_a_second_writer() {
    assert_eq!(after_classic(false, false), AfterClassic::Send);
    assert_eq!(after_classic(false, true), AfterClassic::Send);
    assert_eq!(after_classic(true, true), AfterClassic::Warn);
    assert_eq!(after_classic(true, false), AfterClassic::Respawn);
}
// agent_conversation.rs
#[test] fn pausing_is_one_notice_and_done() {
    // you("x") then a pending Permission; paused() → the card reads Expired, the last item is
    // Notice(PAUSED_NOTICE), no Error row, status Done
}
// pane_tab_menu.rs
#[test] fn an_agent_tab_renames_and_opens_classic_only_with_a_conversation() { /* Rename enabled; OpenClassic enabled iff conversation Some; PopOut disabled */ }
// tab_bar.rs
#[test] fn an_agent_tab_shows_its_badge_title_and_tip() {
    // bar_tabs with agent facts {label:"Claude", badge:"CC", rgb:0x7a4a2a, status:Waiting, age:4 min} and title Some("fix tests")
    // → icon == TabIcon::Agent{badge:"CC",rgb:0x7a4a2a}, title "fix tests", tip "fix tests — waiting 4m"
}
// agent_process.rs
#[test] fn end_all_ends_a_term_ignoring_agent() {
    // two `sh -c 'trap "" TERM; sleep 100'`; end_all(&[a, b], 2 s) returns in < 3 s; then kill(-pgid, 0) fails with ESRCH for both
}
```
  Scenes: `agent-workspace-split` (a shell and a Claude tab side by side, the row reading "Claude"),
  `agent-preset-menu` (badges in the sidebar's preset menu), `agent-pane-menu` (the `···` menu with "New Claude
  tab"), and the chrome scene **`chrome-agent-tabs`** (`chrome_sources.rs`, over `elsewhere_window`, `Seen::Changes`
  each): "an agent tab beside api" (`agent_beside`, the `then` hook's body: api's bar gains a CC tab, the row label is
  unchanged since api is named), "the pane menu adds a Claude tab" (`run_pane_action(NewAgent("claude"))`), "the
  first message titles the tab" (`AgentTab::send` → `Title` → `on_agent_event`), "rename the agent tab" (the rename
  box's Confirm, as `chrome-renaming`), "open it in classic view" (`open_classic`, Idle: no sheet; a terminal tab
  joins the pane), "close the agent tab" (`close_tab`, Idle: no sheet). Register it in `SCENES`.
  Run `cargo test --bin fuxx` and `__ui-shots agent-` / `__ui-shots chrome-agent` → FAIL.
- [ ] **Step 2:** Implement. **Step 3:** tests and scenes pass. **RED:** delete the `cx.notify()` in
  `on_agent_event`'s `Title` arm → `chrome-agent-tabs` "the first message titles the tab" is STALE; restore it.
  Re-run `__ui-shots chrome` (all M11p scenes still pass) and `__ui-shots tab` / `__ui-shots sidebar`.
- [ ] **Step 4:** Full gate (remove `presets.rs:27`'s allow).
- [ ] **Step 5: Commit** `M11aa: agent tabs in the workspace`.

### Task 9: Status, marks, notifications, sidebar, Activity

**Files:** Modify `src/workspace_agent.rs` (`set_agent_status`, `row_winner`, the `Status` / `Question` arms of
`on_agent_event`), `src/sidebar.rs` (`WorkspaceFacts` `:62`, `WorkspaceRow` `:119`, `rows()` `:175`,
`row_subjects` `:499`, `sidebar_rows` `:1217`, `Workspace::rows` `:1314` (age bucket), `marks_on_screen` `:1355`,
`tab_sessions` `:1394`, `refresh_ages` `:1451`, the row's badge `:1893` / `avatar_slot` `:2400`, the Open button
`:2042`, `session_mark_sized` `:2330`), `src/tab_bar.rs` (`render_tab`: the agent's mark after its title),
`src/activity_panel.rs` (`activity_session` `:362` and its users), `src/ui_shots/chrome_sources.rs` (`Steps`' age
helpers `:88`, `:114`, `:133`, `:151`; new scenes), `src/ui_shots.rs` (`chrome-pixels-same` `:2336`).
`set_status` (`workspace.rs:2117`) is the reference; terminals' code is not changed.

**Interfaces:**
- Consumes: `AgentSlot`, `on_agent_event` (Task 8); `AgentTabEvent::{Status, Question}` (Task 7; `on_events` emits
  `Question` before `Status`, so a Waiting notification has its question).
- Produces:

```rust
// workspace_agent.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowWinner { Terminal(usize), Agent(TabId) }
/// The most urgent of a workspace's terminal rollup and its agent tabs (`groups::urgency`); a tie goes to the
/// terminal, then to the first agent tab in reading order. None: neither.
pub fn row_winner(terminal: Option<(usize, Status)>, agents: &[(TabId, Status)]) -> Option<RowWinner>;
impl Workspace {
    /// For an agent tab what `set_status` does for a terminal: status + since, Done pop (live and its row on screen),
    /// Activity entry (`slot.activity`), 🔔 list entry (`terminal: None`), sound + banner via `notifier::{should_notify,
    /// message, body, banner_title, banner_subtitle, worth_a_banner, fire}` with `banner_about(None)` (call 3; subtitle's
    /// agent = slot.label; `doing` = the tab's title), `check_waits(&[], cx)`, then `cx.notify()`. `live: false` (history):
    /// no notification, no pop, no Activity entry.
    pub(crate) fn set_agent_status(&mut self, tab: TabId, new: Status, live: bool, cx: &mut Context<Self>);
    /// The agent tabs of the shown panes, in layout then bar order: (tab, aN, status).
    pub(crate) fn tab_agents(&self) -> Vec<(TabId, u64, Status)>;
}
// sidebar.rs
pub fn row_subjects(model: &Model, index_of: impl Fn(u64) -> Option<usize>, status: impl Fn(u64) -> Status,
                    agents: impl Fn(WorkspaceId) -> Vec<(TabId, Status)>) -> Vec<(WorkspaceId, Option<usize>, Option<TabId>)>;
// WorkspaceFacts / WorkspaceRow gain `pub agent: Option<TabId>` (the winner, when an agent tab); `subject` stays the
// terminal rollup's session; `status`, `since`, `question` are the winner's.
impl Workspace {
    /// The mark drawing of `session_mark_sized`, by parts: `place`/`key` pick the `MarkView`, `pop` the Done pop.
    pub(crate) fn status_mark(&self, place: &'static str, key: u64, status: Status, pop: Option<(u64, Instant)>,
                              ui: UiColors, fill: Rgb, size: f32, cx: &mut Context<Self>) -> AnyElement;
}
```
- `on_agent_event`: `Question(q)` → `slot.question = q` (no notify: the `Status` that follows notifies; a
  `Question` alone — a second permission while Waiting — notifies itself); `Status { status, live }` →
  `set_agent_status`.
- `marks_on_screen` chains `tab_agents()` as `(("agent-tab", aN), status)` (R2); `sync_chrome` then keeps their
  views and the ticker runs for a Working or Waiting agent tab on screen. `render_tab` draws an agent tab's mark
  after its title (when not Idle) with `status_mark("agent-tab", slot.id, …)`.
- Rows: `sidebar_rows` passes each workspace's `(tab, slot.status)` to `row_subjects`; the winner's status, since
  and question fill `WorkspaceFacts`; `avatar_slot` reads `row.status` (it read the session's, the same for a
  terminal winner); the Open button (`:2042`) focuses the agent tab when `row.agent` is Some.
- Ages: `Workspace::rows` builds its age bucket from sessions **and** agent slots not Idle; `refresh_ages` takes a
  row's since from `row.agent`'s slot when Some, else from `row.subject`'s session.
- Activity: `activity_session` becomes `activity_subject()` → the focused agent tab's slot (its log, title and
  status) when an agent tab is focused, else the followed terminal as today.
- `Steps`' age helpers (`clear_of_age_turns`, `ages`, `age_labels`, `past_age_turns`) read agent slots too, or an
  agent's label turning mid-check reads as STALE.

- [ ] **Step 1: Failing tests**

```rust
// workspace_agent.rs
#[test] fn waiting_agent_beats_idle_shell() {
    assert_eq!(row_winner(Some((0, Status::Idle)), &[(TabId(7), Status::Waiting)]), Some(RowWinner::Agent(TabId(7))));
}
#[test] fn a_tie_goes_to_the_terminal() {
    assert_eq!(row_winner(Some((0, Status::Done)), &[(TabId(7), Status::Done)]), Some(RowWinner::Terminal(0)));
}
#[test] fn no_agents_is_the_terminal() { assert_eq!(row_winner(Some((2, Status::Working)), &[]), Some(RowWinner::Terminal(2))); }
#[test] fn an_agent_only_workspace_has_an_agent_subject() {
    assert_eq!(row_winner(None, &[(TabId(3), Status::Idle)]), Some(RowWinner::Agent(TabId(3))));
}
// sidebar.rs
#[test] fn an_agent_winner_row_reads_its_status_age_and_question() {
    // facts with agent: Some(TabId(7)), status Waiting, since 5 min ago, question Some("Claude wants to run: ls")
    // → WorkspaceRow { agent: Some(TabId(7)), status: Waiting, age: Some("5m"), sub: Some(Sub::Question(..)), .. }
}
```
  Scenes (`chrome_sources.rs`, registered in `SCENES`): **`chrome-agent-status`** — `elsewhere_window` plus a Claude
  tab beside `api` (shown) and one beside `agent` (not shown), each step by `shot_feed` (the decoded-event path) or
  `AgentTab::answer`, `Seen::Changes`: "agent's Claude works (elsewhere)" (its row's ring), "api's Claude works"
  (its tab mark), "the ticker turns api's Claude mark" (`advance_clock(TICK)` ×3), "agent's Claude asks" (a
  Permission: row red, the question under the name, 🔔), "it is allowed" (`answer(Allowed)`: Working), "api's Claude
  is done" (`TurnDone`: the Done pop), "api's Claude idles in a closed group" (`Seen::Maybe`).
  **`chrome-agent-age`** — agent's Claude Working, dated minutes back by a ui-shots hook that notifies (as
  `minutes_old`, `chrome_sources.rs:322`), then nothing but the age's own timer: the row's age label turns and the
  frame equals a full redraw (as `chrome-ages-current`, `ui_shots.rs:1494`). `chrome-pixels-same` gains a Working
  agent tab (cached = uncached). Look scene `agent-sidebar-waiting` (red ring on the row).
  Run → FAIL.
- [ ] **Step 2:** Implement. **Step 3:** tests and scenes pass. **RED ×3** (each restored after): delete the
  `cx.notify()` at the end of `set_agent_status` → `chrome-agent-status` STALE; drop agent slots from
  `refresh_ages` → `chrome-agent-age` STALE (or its label never turns); drop `tab_agents()` from
  `marks_on_screen` → "the ticker turns api's Claude mark" STALE or unchanged. Re-run `__ui-shots chrome` (every
  M11p scene still passes). Full gate.
- [ ] **Step 4:** Note for Task 13's checklist: a permission prompt while another workspace is focused → ring,
  🔔 entry, banner (manual; the app is not launched here).
- [ ] **Step 5: Commit** `M11aa: agent tab status, notifications and sidebar`.

### Task 10: Scripting API and MCP see agent tabs

**Files:** Modify `src/api.rs` (`Target` `:109`, `parse_target` `:118`, `resolve` `:147`), `src/api_events.rs`
(`EventSession` `:16` and its JSON `:52`, `Watch` `:60`, `Outcome` `:68` and `wait_reply` `:235`, the wait check),
`src/workspace_api.rs` (`session.list` `:273` / `list` `:420`, `.status` `:330`, `.question` `:334`, `.answer`
`:343`, `.wait` `:229`, `send` / `key` / `paste` / `read` targets (`input_target` `:483`, `target_index` `:492`),
`check_waits` `:589`, `statuses` `:634`, `emit` `:644`), `src/workspace_agent.rs` (`set_agent_status` emits),
`src/mcp.rs` (tool descriptions `:55-95` mention `aN`).

**Interfaces:**
- Produces:

```rust
// api_events.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject { Terminal(u64), Agent(u64) }      // Display: "s3" / "a3"
pub enum Watch { One(Subject), Any }
pub enum Outcome { Met(Subject), Closed(Subject), TimedOut }
pub struct EventSession<'a> { pub subject: Subject, pub number: usize, pub name: &'a str }   // JSON "id": subject.to_string()
// api.rs
pub enum Target { Id(u64), Agent(u64), Number(usize), Name(String) }   // "a<digits>" → Agent
pub fn answer_action(text: &str) -> Answer;   // y / yes / allow (any case, trimmed) → Allowed; always → AlwaysAllowed; else Denied(Some(text))
// workspace_api.rs
fn statuses(&self) -> Vec<(Subject, Status)>;   // terminals in sidebar order, then agent tabs by workspace order
pub(crate) fn emit_agent(&mut self, event: &Event, tab: TabId);   // EventSession { subject: Agent(aN), number: its row, name: its title }
```
  `session.list` rows for agent tabs: `{"id":"aN","kind":"agent","provider":"claude","name":<title or label>,
  "status":…,"workspace":…,"pane":…}`; terminal rows gain `"kind":"terminal"`. `session.status` on `aN` → the
  slot's status. `session.question` on `aN` → `{"text": AgentTab::question()}`, error `not_waiting` when not
  Waiting. `session.answer` on `aN` → `AgentTab::answer(answer_action(text))` (its `Err("not waiting")` →
  `not_waiting`). `send`, `key`, `paste`, `read` on `aN` → error `"aN is an agent tab, not a terminal"`. No-target
  resolution is unchanged (the focused terminal). `set_agent_status` (live) emits `StatusChanged` and
  `Notification` through `emit_agent`; a closed agent tab passes `Subject::Agent(aN)` in `check_waits`' `closed`.
  A wait `--for` text on an agent tab never matches (no screen).

- [ ] **Step 1: Failing tests** (pure)

```rust
#[test] fn agent_targets_parse() {
    assert_eq!(parse_target("a3"), Target::Agent(3));
    assert_eq!(parse_target("s3"), Target::Id(3));
    assert_eq!(parse_target("a"), Target::Name("a".into()));
}
#[test] fn answer_words_map_to_actions() {
    assert_eq!(answer_action(" Yes "), Answer::Allowed);
    assert_eq!(answer_action("always"), Answer::AlwaysAllowed);
    assert_eq!(answer_action("not now"), Answer::Denied(Some("not now".into())));
}
#[test] fn wait_any_sees_agent_waiting() { /* Watch::Any for waiting is Met(Subject::Agent(1)) when statuses gain (Agent(1), Waiting) */ }
#[test] fn subjects_print_as_ids() { assert_eq!(Subject::Agent(2).to_string(), "a2"); assert_eq!(Subject::Terminal(2).to_string(), "s2"); }
#[test] fn list_marks_terminal_rows() { /* list_entry(..) JSON has "kind":"terminal" */ }
```
- [ ] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** → PASS; full gate. Note for Task 13's checklist:
  `fuxx status a1`; `fuxx wait --any --for waiting` returns when the agent asks; `fuxx answer a1 yes` allows it.
- [ ] **Step 5: Commit** `M11aa: agent tabs in the scripting API`.

### Task 11: Drag a command block onto an agent tab

**Files:** Modify `src/block_view.rs` (`BlockDrag`, `attachment_text`, beside `with_command` `:357`),
`src/terminal_view.rs` (`chip_element` `:1699`: the chip's command text is the drag handle), `src/agent_tab.rs`
(drop on the body), `src/tab_bar.rs` (`render_tab`: an agent tab accepts the drop), `src/workspace_agent.rs`.

**Interfaces:**
- Produces:

```rust
// block_view.rs
#[derive(Clone)]
pub struct BlockDrag { pub id: String, pub command: Option<String>, pub outcome: Outcome, pub core: TerminalCore }
pub const ATTACH_CAP: usize = 64 * 1024;
/// "$ {command}\n(exit {code})\n{output}": no "$" line without a command, no exit line unless `Failed(code)` or
/// `Ok` (exit 0); the output's tail kept to ATTACH_CAP bytes (on a char boundary) under a first line
/// "… (earlier output cut)".
pub fn attachment_text(command: Option<&str>, outcome: Outcome, output: &str) -> String;
// workspace_agent.rs
pub(crate) fn attach_block(&mut self, tab: TabId, drag: &BlockDrag, cx: &mut Context<Self>);
```
- The chip's command text gets `on_drag(BlockDrag { .. }, …)` (a chip like `TabChip` follows the pointer, as
  `tab_bar.rs:622` does for tabs). A drop on the agent tab's body (`on_drop::<BlockDrag>` in `AgentTab::render`) or
  on its tab in a bar (`render_tab`) calls `attach_block`: the output is read on the background executor
  (`core.block_output(&id)`, as `copy_block` does, `terminal_view.rs:1379`) and then
  `AgentTab::attach(attachment_text(..))`. Hover feedback uses GPUI's element-local `drag_over::<BlockDrag>` style
  only — **no Workspace field changes during a drag-over** (a cached tab bar wouldn't show it). Paste into the
  composer is unchanged. Attachments show as chips under the composer until sent, then under the `You` card.

- [ ] **Step 1: Failing test:** `attachment_text_formats_and_caps`:

```rust
let out = "x".repeat(100 * 1024);
let t = attachment_text(Some("npm test"), Outcome::Failed(1), &out);
assert!(t.starts_with("$ npm test\n(exit 1)\n… (earlier output cut)\n"));
assert!(t.len() <= ATTACH_CAP + 64);
assert_eq!(attachment_text(None, Outcome::Unknown, "ok"), "ok");
```
- [ ] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** scene `agent-tab-attachment` (a dropped block's chip in
  the composer, then sent under the `You` card, by `attach_block`'s path with a shot terminal's block); full gate.
  Note for Task 13's checklist: drag a failed `npm test` block onto the agent.
- [ ] **Step 5: Commit** `M11aa: attach a command block to an agent`.

### Task 12: The agy provider

**Files:** Create `src/providers/agy.rs`; modify `src/providers/mod.rs` (`all()` `:134`),
`src/providers/fixtures/agy-*.jsonl` (re-recorded), `scripts/record-agent-fixture.py` if agy's scenarios need it.

- [ ] **Step 0: Re-record the agy fixtures first** (Task 0 hit "Your AI credits balance is too low to continue",
  429 RESOURCE_EXHAUSTED, so every saved `agy-*.jsonl` is an error). Run the recorder for `agy text`, `agy tools`,
  `agy resume` with the argv in Global Constraints. **If agy still fails** (credits, login, a changed schema):
  write what it printed into the proposal's agy section and the task report, leave this task's other steps
  unchecked, commit `M11aa: agy fixtures still unusable (Task 12 parked)`, and go on to Task 13 — Task 13 does not
  depend on agy; the built-in `agy` preset (Task 1) then has no provider and stays a classic preset
  (`launch_plan_for` → `Classic`), which Task 13's note records.

**Interfaces:**
- Produces: `pub static AGY: Agy;` — `ProviderInfo { name: "agy", label: "Antigravity", badge: "AG", badge_rgb:
  0x2d4f8a, routes_permissions: false, modes: &["accept-edits","auto","plan"], default_mode: Some("accept-edits") }`;
  argv per Global Constraints (`--mode` from `start.mode`, else the default); `classic_argv(id)` =
  `["agy","--conversation",id]`. **Its own decoder:** `init` → `Started { conversation: conversation_id, model: None }`;
  `step_update` with `step_type: "agent_response"` and `text_delta` → `TextDelta { message: "<conversation>:<step_index>" }`,
  its `state: "DONE"` → `MessageDone`; `error_message` → `Error`; tool steps → `ToolStarted` / `ToolFinished` by
  whatever the re-recorded `agy-tools.jsonl` shows (write the mapping into the task report); `result` → `Usage`
  then `TurnDone { ok: status == "SUCCESS", message: error }`. `hello()` is empty; `encode(Send)` writes
  `{"event":"user","message":{"content":…}}`; `Allow` / `AlwaysAllow` / `Deny` encode nothing (no routing);
  `Interrupt` → nothing unless the recording shows a way (then say which). Pass-through drops `-p`, `--print`,
  `--input-format`, `--output-format`, `--conversation`, `--continue`, `--mode` and
  `--dangerously-skip-permissions`. Registering `&agy::AGY` in `all()` makes the `agy` preset a provider preset
  (badge in menus, `···` "New Antigravity tab"), with no other file changed.
- [ ] **Step 1: Failing tests:** `agy_text_fixture_streams_then_finishes` (Started first, a TextDelta, TurnDone ok
  last), `agy_tools_fixture_pairs_tools` (every ToolStarted id has one ToolFinished), `agy_errors_map_to_an_error_row`
  (the old 429 recording, kept as `agy-credits.jsonl`: an `Error` then `TurnDone { ok: false }`),
  `agy_argv_has_mode_and_never_skips_permissions` (`--mode accept-edits` by default; `--dangerously-skip-permissions`
  absent even when in `extra_args`), `agy_resume_uses_conversation_flag`, and in `mod.rs` `by_name("agy")` is agy.
- [ ] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** → PASS; scene `agent-tab-agy` (AG badge, the mode chip
  before the first message, read-only after); `__ui-shots chrome-agent` still passes. Full gate.
- [ ] **Step 5: Commit** `M11aa: agy provider`.

### Task 13: Docs and the on-screen checklist

**Files:** Create `docs/04-Development/79 - Milestone 11aa - Agent Tabs.md` (the next free number after `78`; the
proposal's Part 1 moves here; the proposal keeps Part 2 for M11ab); modify `40 - Roadmap.md`,
`60 - Progress Log.md` (an entry, and the "Current status snapshot"), this plan ("At a glance", checkboxes).

- [ ] **Step 1:** `rg -n "allow\(dead_code\)" src/providers src/agent_*.rs src/composer.rs src/presets.rs` → nothing
  (or each hit names a reason the review accepted). Full gate; record the test count and both clippy counts.
- [ ] **Step 2:** Milestone note: goal, what was built (per task), the refresh calls R1–R12, limits (incl. Ruling 17's
  straggler, agy's state if Task 12 parked, Mermaid shown as code in chat), and an **on-screen checklist** for the
  user, at least: ⌃⌘1 opens a shell and Claude side by side and the row reads "Claude"; a streamed reply with
  Markdown (list, code, table); a tool row opens to a diff; a Write asks and Allow / Allow all edits / Deny each
  work; Esc stops ("Stopping…", Force stop after 10 s); ring + mark + 🔔 + banner when it asks while another
  workspace is focused; the agent tab's mark turns while it works, and its tab title is your first message; rename
  the tab; close a Working agent tab → asked, and afterwards `pgrep -f "claude -p"` finds nothing; quit and relaunch
  restores the conversation (history shown) and the shell does not type `claude`; typing `claude` in the shell is
  still the classic TUI; Open in classic view pauses the tab (its notice row), and sending from the tab while
  `claude` still runs there warns instead of starting a second one, and after you quit it resumes; an agy tab in each mode (if Task 12 shipped); drag a failed
  `npm test` block onto the agent; `fuxx status a1`, `fuxx wait --any --for waiting`, `fuxx answer a1 yes`;
  `FUXX_CHROME_CACHE=off` looks identical with an agent tab open.
- [ ] **Step 3:** Roadmap status "🟨 built, on-screen check pending" (M11ad stays next, design first); Progress Log
  entry with the test count.
- [ ] **Step 4: Commit** `M11aa: docs`.
