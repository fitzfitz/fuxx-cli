---
tags: [development, plan, milestone]
milestone: M9
status: done
updated: 2026-10-27
---

# M9 Plan — Scripting API (socket, `fuxx` command, events, wait, MCP)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[49 - Milestone 9 - Scripting API]] (the spec) |
| **Status** | ✅ Done (2026-10-27). All on-screen checks confirmed by you. |
| **Tasks done** | 10 / 10 |
| **▶ Pick up here** | — (done) |
| **Decisions used** | **Yours (2026-10-27):** "go ahead", and more functionality than cmux / Warp / iTerm2. Everything else is the AI's call, listed in the spec. |
| **Needs your eyes** | 👀 Task 9 |

**Goal:** A same-user local socket with a JSON-lines protocol, a `fuxx <command>` CLI, agent-aware commands
(status / question / answer / wait), an event stream, sidebar note + progress, and `fuxx mcp`.

**Architecture:**

```
 fuxx <command>  ──► cli.rs (pure: argv → Request, Result → text, exit codes) ──► api_client.rs (UnixStream)
 fuxx mcp        ──► mcp.rs (pure: MCP JSON-RPC ↔ Request) ──────────────────────┘          │
                                                                                            ▼
 api_server.rs (socket thread per connection; same-user check) ── Incoming{request, conn} ──► Workspace (UI thread)
 api.rs (pure: Request/Response/ApiError/Event, Target parse+resolve, key names → bytes, WaitFor)   handles, answers,
                                                                                                    keeps waits + subscribers
```

**Tech Stack:** No new crates (serde/serde_json from M7, futures channel from M1). Unix sockets via
`std::os::unix::net::{UnixListener, UnixStream}`; same-user check via libc `getpeereid` / `getuid` (FFI, as fonts.rs
does for CoreText). MCP protocol **2025-11-25**, stdio transport: newline-delimited JSON-RPC 2.0; `initialize` →
`{protocolVersion, capabilities: {tools: {}}, serverInfo}`; `tools/list`; `tools/call` → `{content: [{type: "text",
text}], isError}` (checked via Context7, 2026-10-27).

**Spec:** [[49 - Milestone 9 - Scripting API]] · [[12 - Scope]]

## Global Constraints

- **Inherited:** everything from the M0–M8 plans. **No new crates.**
- **Gates:** 0 warnings from our code; `cargo fmt --check` clean; every test passes (start: 150).
- **Behaviour:** exactly the spec's "How it behaves" and nothing under "Not in M9".
- **Socket:** `~/Library/Application Support/fuxx/fuxx.sock`. Its folder is created `0700` and the socket set `0600`. Every accepted connection must pass `getpeereid` uid == `getuid()`, or it's closed with no reply. `scripting = off` (settings, read at startup) means no socket.
- **Protocol:** one JSON object per line, UTF-8. Request `{"id"?: u64, "method": str, "params"?: obj}`; reply `{"id", "result"}` or `{"id", "error": {"code", "message"}}`. Error codes: `bad_request`, `unknown_method`, `not_found`, `ambiguous`, `refused`, `timeout`, `unavailable`. Events: `{"event": str, "session"?: {id, number, name}, …, "ts": unix-millis}`.
- **Ids:** `s<N>`, a per-launch counter that starts at 1 and is never reused.
- **CLI exit codes:** 0 ok · 1 refused / nothing to show / other API error · 2 bad arguments · 3 can't reach fuxx · 124 wait timed out.
- **Shell env:** `FUXX_SOCKET`, `FUXX_SESSION=s<N>`, and `PATH` with the current exe's folder appended. Only when scripting is on.
- **Never blocks the UI thread** on socket I/O: the socket threads write replies; the UI thread only queues lines.
- **No `cargo run` by subagents.** GUI behaviour is checked by the user in Task 9. Socket/CLI pieces are tested against a temporary socket path.

## Review Focus

1. **Another local user (or a stale socket) gets in, or a crash leaves fuxx unable to start** → `bind_makes_a_private_socket_and_replaces_a_stale_one`, `peer_check_accepts_only_the_same_user` (Task 3).
2. **The wrong session gets typed into** (ambiguous name, a target resolved against a list that changed) → `resolve_*` tests (Task 1); the workspace resolves at handling time, by id.
3. **A slow or dead client freezes fuxx** (a subscriber that never reads, a client that disconnects mid-wait) → replies go through per-connection writer threads with a bounded queue; a failed write drops the subscriber/wait (Tasks 3, 6).
4. **`answer` types into a session that isn't waiting** → refused unless `force` (Task 5); `question` returns nothing unless waiting.
5. **Enter isn't Enter for agents** (Claude Code needs `\r`, not `\n`) → `key_bytes("enter") == "\r"` and `--enter` appends `\r` (Tasks 1, 2).

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `src/api.rs` | Create | Protocol types, `Target`, `resolve`, `key_bytes`, `WaitFor`, `Status` names | 1 |
| `src/cli.rs` | Create | argv → `Cli`, result → text, exit codes | 2 |
| `src/api_server.rs`, `src/api_client.rs` | Create | Socket server (threads, peer check, writers) and client | 3 |
| `src/main.rs` | Modify | CLI dispatch before the GUI; start the server | 3 |
| `src/appearance.rs` | Modify | `scripting: bool` from `scripting = on|off` | 3 |
| `src/terminal.rs`, `src/terminal_view.rs`, `src/grid.rs`, `src/pty.rs` | Modify | Read text (screen / last N / all), send bytes/paste, env vars | 4 |
| `src/workspace.rs` | Modify | Session ids, request handling, events, waits, note/progress | 4–7 |
| `src/mcp.rs` | Create | MCP server (pure handler + stdio loop) | 8 |

---

### Task 1: The protocol, without I/O — ✅

**Files:** Create `src/api.rs` · Modify `src/main.rs` (`mod api;`), `src/status.rs` (names).

**Interfaces (produces):**
- `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct Request { #[serde(default, skip_serializing_if = "Option::is_none")] pub id: Option<u64>, pub method: String, #[serde(default)] pub params: serde_json::Value }`
- `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct ApiError { pub code: &'static str, pub message: String }` with constructors `ApiError::bad_request(msg)`, `not_found`, `ambiguous`, `refused`, `unknown_method`, `timeout`, `unavailable`.
- `pub fn reply(id: Option<u64>, result: Result<serde_json::Value, ApiError>) -> String`: one line (no trailing newline), `{"id":…,"result":…}` or `{"id":…,"error":{"code","message"}}`; `id` is `null` when absent.
- `Status::as_str(self) -> &'static str` (`idle|working|waiting|done`) and `Status::parse(&str) -> Option<Status>` in `status.rs`.
- `#[derive(Debug, Clone, PartialEq)] pub enum Target { Id(u64), Number(usize), Name(String) }` and `pub fn parse_target(s: &str) -> Target`: `s<digits>` → `Id`, `#<digits>` → `Number` (1-based), else `Name`.
- `#[derive(Debug, Clone)] pub struct SessionRef { pub id: u64, pub label: String }` (in sidebar order).
- `pub fn resolve(target: Option<&Target>, sessions: &[SessionRef], caller: Option<u64>, focused: Option<u64>) -> Result<u64, ApiError>`:
  - `None` → `caller` if it's in `sessions`, else `focused`, else `not_found`.
  - `Id` → must exist.
  - `Number(n)` → `sessions[n-1]`.
  - `Name` → case-insensitive equality with `label`. 0 matches → `not_found`; >1 → `ambiguous`, naming the ids.
- `pub fn key_bytes(name: &str, app_cursor: bool) -> Option<Vec<u8>>`: names `enter`→`\r`, `tab`, `esc`/`escape`, `backspace`, `delete`, `up/down/left/right`, `home/end`, `pageup/pagedown`, `f1`–`f12`, `space`, and `ctrl-<x>` / `alt-<x>` / `shift-<key>` combos. Built on `keys::key_to_bytes`. Unknown → `None`.
- `#[derive(Debug, Clone, PartialEq)] pub enum WaitFor { Status(Status), Change, Exit, Text(String) }` with `pub fn parse_wait(for_: Option<&str>, text: Option<&str>) -> Result<WaitFor, ApiError>`. `text` wins; otherwise `for_` defaults to `waiting`; `change`, `exit`, or a status name.
- `pub fn wait_met(cond: &WaitFor, start: Status, now: Status, screen: &str, exited: bool) -> bool`. `Exit` → exited. `Change` → exited or now != start. `Status(s)` → now == s. `Text(t)` → screen contains t.
- `pub const METHODS: &[&str]` = every method name in the spec's protocol list.

- [ ] **Step 1: Failing tests** (in `api.rs`):

```rust
fn refs() -> Vec<SessionRef> {
    vec![
        SessionRef { id: 1, label: "🖕".into() },
        SessionRef { id: 4, label: "api".into() },
        SessionRef { id: 7, label: "🖕".into() },
    ]
}

#[test]
fn parse_target_kinds() {
    assert_eq!(parse_target("s4"), Target::Id(4));
    assert_eq!(parse_target("#2"), Target::Number(2));
    assert_eq!(parse_target("api"), Target::Name("api".into()));
    assert_eq!(parse_target("s"), Target::Name("s".into()));
}

#[test]
fn resolve_by_id_number_name_and_default() {
    let r = refs();
    assert_eq!(resolve(Some(&Target::Id(4)), &r, None, None), Ok(4));
    assert_eq!(resolve(Some(&Target::Number(3)), &r, None, None), Ok(7));
    assert_eq!(resolve(Some(&Target::Name("API".into())), &r, None, None), Ok(4));
    assert_eq!(resolve(None, &r, Some(7), Some(1)), Ok(7));
    assert_eq!(resolve(None, &r, Some(99), Some(1)), Ok(1)); // caller gone: the focused one
}

#[test]
fn resolve_errors() {
    let r = refs();
    assert_eq!(resolve(Some(&Target::Id(9)), &r, None, None).unwrap_err().code, "not_found");
    assert_eq!(resolve(Some(&Target::Number(0)), &r, None, None).unwrap_err().code, "not_found");
    let e = resolve(Some(&Target::Name("🖕".into())), &r, None, None).unwrap_err();
    assert_eq!(e.code, "ambiguous");
    assert!(e.message.contains("s1") && e.message.contains("s7"));
    assert_eq!(resolve(None, &[], None, None).unwrap_err().code, "not_found");
}

#[test]
fn key_names_to_bytes() {
    assert_eq!(key_bytes("enter", false), Some(b"\r".to_vec()));
    assert_eq!(key_bytes("ctrl-c", false), Some(vec![3]));
    assert_eq!(key_bytes("up", false), Some(b"\x1b[A".to_vec()));
    assert_eq!(key_bytes("up", true), Some(b"\x1bOA".to_vec()));
    assert_eq!(key_bytes("esc", false), Some(vec![0x1b]));
    assert_eq!(key_bytes("f5", false), Some(b"\x1b[15~".to_vec()));
    assert_eq!(key_bytes("nope", false), None);
}

#[test]
fn wait_conditions() {
    assert_eq!(parse_wait(None, None), Ok(WaitFor::Status(Status::Waiting)));
    assert_eq!(parse_wait(Some("done"), Some("ok")), Ok(WaitFor::Text("ok".into())));
    assert_eq!(parse_wait(Some("bogus"), None).unwrap_err().code, "bad_request");
    assert!(wait_met(&WaitFor::Status(Status::Done), Status::Working, Status::Done, "", false));
    assert!(!wait_met(&WaitFor::Change, Status::Working, Status::Working, "", false));
    assert!(wait_met(&WaitFor::Change, Status::Working, Status::Working, "", true));
    assert!(wait_met(&WaitFor::Text("PASS".into()), Status::Idle, Status::Idle, "tests PASS\n", false));
}

#[test]
fn reply_shapes() {
    assert_eq!(reply(Some(1), Ok(serde_json::json!({"pong": true}))), r#"{"id":1,"result":{"pong":true}}"#);
    assert_eq!(
        reply(None, Err(ApiError::not_found("no session s9"))),
        r#"{"id":null,"error":{"code":"not_found","message":"no session s9"}}"#
    );
}
```

- [ ] **Step 2: Run** `cargo test api`. Expected: compile errors.
- [ ] **Step 3: Implement.** GPUI-free. Narrow `#[allow(dead_code)]` until Task 5 (ledger ruling, as in M7/M8).
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (156), 0 warnings.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m9): scripting protocol types, targets, key names, wait conditions"`

### Task 2: The `fuxx` command line, without I/O — ✅

**Files:** Create `src/cli.rs` · Modify `src/main.rs` (`mod cli;`).

**Interfaces (produces):**
- `pub enum Cli { Gui, Mcp, Help(String), Call { request: Request, json: bool, stream: bool } }`
- `pub fn parse(args: &[String], caller: Option<u64>) -> Result<Cli, String>` (args exclude argv[0]):
  - No args, or a first arg that isn't a command name → `Gui`. Macs can pass `-psn_…`.
  - `help` / `--help` → `Help(usage)`. `mcp` → `Mcp`.
  - Otherwise each spec command → its method with params. `target` is an optional string: a positional first argument when the command takes `[TARGET] X` and two positionals are given. For `key`, the first argument is the target only if it isn't a key name.
  - `caller` (from `FUXX_SESSION`, parsed as `s<N>`) is added as `"caller": N` to every request.
  - `--json` sets `json`. `events` and `wait` set `stream` (the client reads until the reply line or end).
  - Bad usage → `Err(message)`.
- Params (exact keys):
  - `session.new` takes `folder`, `preset`, `run`, `name`, `split` (`"right"`/`"down"`), `of`, `background`.
  - `session.send` takes `text` and `enter`; `session.key` takes `keys: [..]`; `session.paste` takes `text` (the CLI reads stdin).
  - `session.read` takes `lines?` and `all`; `session.answer` takes `text` and `force`.
  - `session.wait` takes `any`, `for?`, `text?` and `timeout_ms?`.
  - `session.note` takes `text` (null clears); `session.progress` takes `percent` (null clears) and `label?`.
  - `notify` takes `title?` and `body`.
- `pub fn render(command: &str, result: &serde_json::Value, json: bool) -> String`. JSON mode prints the result compactly. Text mode:
  - `ls`: one line per session, `s4  #2  waiting  api  [~/w/api]  (claude)`.
  - `new` and `wait`: the id.
  - `read` and `question`: the text as is.
  - `status`: the word.
  - `events`: `12:01:03  s4 #2 api  working → waiting`.
  - Others: nothing on success.
- `pub fn exit_code(error_code: &str) -> i32` per Global Constraints.

- [ ] **Step 1: Failing tests:**

```rust
fn p(a: &[&str]) -> Result<Cli, String> {
    parse(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>(), Some(3))
}
fn call(c: Result<Cli, String>) -> (Request, bool, bool) {
    match c.unwrap() {
        Cli::Call { request, json, stream } => (request, json, stream),
        _ => panic!("not a call"),
    }
}

#[test]
fn no_args_or_unknown_first_arg_opens_the_app() {
    assert!(matches!(p(&[]), Ok(Cli::Gui)));
    assert!(matches!(p(&["-psn_0_123"]), Ok(Cli::Gui)));
    assert!(matches!(p(&["mcp"]), Ok(Cli::Mcp)));
}

#[test]
fn ls_and_caller() {
    let (r, json, _) = call(p(&["ls", "--json"]));
    assert_eq!(r.method, "session.list");
    assert!(json);
    assert_eq!(r.params["caller"], 3);
}

#[test]
fn send_with_and_without_target() {
    let (r, _, _) = call(p(&["send", "hi"]));
    assert_eq!((r.method.as_str(), &r.params["text"], &r.params["enter"]), ("session.send", &"hi".into(), &false.into()));
    assert!(r.params["target"].is_null());
    let (r, _, _) = call(p(&["send", "s4", "hi there", "--enter"]));
    assert_eq!((&r.params["target"], &r.params["enter"]), (&"s4".into(), &true.into()));
}

#[test]
fn key_target_only_when_not_a_key() {
    let (r, _, _) = call(p(&["key", "ctrl-c", "enter"]));
    assert!(r.params["target"].is_null());
    assert_eq!(r.params["keys"], serde_json::json!(["ctrl-c", "enter"]));
    let (r, _, _) = call(p(&["key", "#2", "up"]));
    assert_eq!(r.params["target"], "#2");
}

#[test]
fn new_and_wait_options() {
    let (r, _, _) = call(p(&["new", "--preset", "claude", "--split", "right", "--background"]));
    assert_eq!(r.method, "session.new");
    assert_eq!((&r.params["preset"], &r.params["split"], &r.params["background"]), (&"claude".into(), &"right".into(), &true.into()));
    let (r, _, stream) = call(p(&["wait", "--any", "--for", "done", "--timeout", "30"]));
    assert!(stream);
    assert_eq!((&r.params["any"], &r.params["for"], &r.params["timeout_ms"]), (&true.into(), &"done".into(), &30000.into()));
    assert!(p(&["new", "--split", "sideways"]).is_err());
    assert!(p(&["send"]).is_err());
}

#[test]
fn exit_codes() {
    assert_eq!((exit_code("refused"), exit_code("timeout"), exit_code("bad_request"), exit_code("unavailable")), (1, 124, 2, 3));
}
```

- [ ] **Step 2: Run** `cargo test cli`. Expected: compile errors.
- [ ] **Step 3: Implement.** Hand-written argument parsing (no crate).
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (162), 0 warnings.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m9): fuxx command-line parsing and output"`

### Task 3: The socket — server, client, and starting it — ✅

**Files:** Create `src/api_server.rs`, `src/api_client.rs` · Modify `src/main.rs`, `src/appearance.rs`.

**Interfaces:**
- `pub fn socket_path(home: &Path) -> PathBuf` → `home/Library/Application Support/fuxx/fuxx.sock`.
- `#[derive(Clone)] pub struct Conn` — a handle to one client connection: `pub fn send_line(&self, line: &str) -> bool`. It queues to that connection's writer thread (bounded, 256 lines); false once the connection is gone or the queue is full, and then the connection is closed. `pub fn is_open(&self) -> bool`.
- `pub struct Incoming { pub request: Request, pub conn: Conn }`.
- `pub fn start(path: &Path) -> std::io::Result<(ServerGuard, futures::channel::mpsc::UnboundedReceiver<Incoming>)>`:
  - Creates the folder `0700`.
  - If `path` exists: a successful `system.ping` to it → `Err(AddrInUse)`; otherwise remove the stale file.
  - Binds, then sets `0600`. The accept thread checks each peer with `getpeereid` == `getuid` (otherwise drops it).
  - One reader thread per connection: each non-empty line is parsed into a `Request` and forwarded. A bad JSON line gets `reply(None, Err(bad_request))` straight from that thread.
  - `ServerGuard` removes the socket file on drop.
- `api_client`:
  - `pub fn call(path: &Path, request: &Request) -> Result<serde_json::Value, ClientError>` sends one line and reads one reply line.
  - `pub fn stream(path: &Path, request: &Request, on_line: impl FnMut(&str) -> bool) -> Result<(), ClientError>` sends and passes every line to `on_line` until it returns false or the connection closes.
  - `ClientError` is either `Unreachable(String)` (exit 3) or `Api { code, message }`.
- `appearance`: `Appearance` gains `pub scripting: bool` from the last `scripting` entry (`on`/`off`, default on; anything else is warned, as with `session-restore`).
- `main.rs`:
  1. Parse `cli::parse(args, env FUXX_SESSION)` **before** creating the application.
  2. `Cli::Call` → run the client: `call`, or `stream` for `events`/`wait`. For `paste`, read stdin into `text` first. Print `render`, then `process::exit(code)`.
  3. `Cli::Help` → print usage. `Cli::Mcp` → `mcp::run()` (Task 8; until then print "not yet" and exit 2).
  4. `Cli::Gui` → today's startup, plus: if `scripting`, `api_server::start`. Keep the guard alive in a global and hand the receiver to the workspace (Task 5 consumes it; until then log `[fuxx] scripting: listening on …`). Failure → one `[fuxx] scripting: …` line, and the app runs without it.

- [ ] **Step 1: Failing tests** (`api_server.rs`, using a temp folder per test under `std::env::temp_dir()`; a server answering pings is built from `start` plus a thread that replies to each `Incoming` with `reply(id, Ok(json!({"pong": true})))`):

```rust
#[test]
fn bind_makes_a_private_socket_and_replaces_a_stale_one() {
    let dir = temp_dir("bind");
    let path = dir.join("f.sock");
    std::fs::write(&path, b"stale").unwrap(); // a leftover file, nobody listening
    let (_guard, _rx) = start(&path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
}

#[test]
fn a_second_server_on_a_live_socket_is_refused() {
    let path = temp_dir("live").join("f.sock");
    let (_g, rx) = start(&path).unwrap();
    pong_forever(rx);
    assert_eq!(start(&path).err().unwrap().kind(), std::io::ErrorKind::AddrInUse);
}

#[test]
fn round_trip_and_bad_json() {
    let path = temp_dir("rt").join("f.sock");
    let (_g, rx) = start(&path).unwrap();
    pong_forever(rx);
    let req = Request { id: Some(5), method: "system.ping".into(), params: serde_json::Value::Null };
    assert_eq!(api_client::call(&path, &req).unwrap(), serde_json::json!({"pong": true}));
    let mut s = std::os::unix::net::UnixStream::connect(&path).unwrap();
    use std::io::{BufRead, Write};
    s.write_all(b"not json\n").unwrap();
    let mut line = String::new();
    std::io::BufReader::new(&s).read_line(&mut line).unwrap();
    assert!(line.contains("\"bad_request\""));
}

#[test]
fn peer_check_accepts_only_the_same_user() {
    assert!(same_user(unsafe { getuid() }));
    assert!(!same_user(unsafe { getuid() } + 1));
}

#[test]
fn unreachable_socket_is_exit_3() {
    let err = api_client::call(&temp_dir("none").join("x.sock"), &Request { id: None, method: "system.ping".into(), params: serde_json::Value::Null }).unwrap_err();
    assert!(matches!(err, api_client::ClientError::Unreachable(_)));
}
```

- [ ] **Step 2: Run** `cargo test api_server`. Expected: compile errors.
- [ ] **Step 3: Implement** (`same_user(peer_uid: u32) -> bool` is the testable half of the peer check).
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (167), 0 warnings.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m9): private scripting socket, client, and CLI dispatch"`

### Task 4: Session ids, shell environment, reading and typing — ✅

**Files:** Modify `src/grid.rs`, `src/terminal.rs`, `src/terminal_view.rs`, `src/pty.rs`, `src/workspace.rs`.

**Interfaces:**
- `grid::text_lines<L>(term: &Term<L>, scope: TextScope) -> String`, with `pub enum TextScope { Screen, Last(usize), All }`.
  - Plain text, one line per row; wrapped rows are joined into one line; trailing spaces trimmed.
  - Wide-char spacers skipped.
  - `Last(n)`: the last n non-blank lines of history + screen. `All`: history then screen. Leading and trailing blank lines are dropped.
- `TerminalCore::text(&self, scope) -> String`; `TerminalView::text(&self, scope)`, `TerminalView::type_bytes(&self, bytes)` (same as keyboard input: snaps to bottom), `TerminalView::paste(&self, text)` (bracketed when the mode asks; reuses `keys::paste_bytes`), `TerminalView::app_cursor(&self) -> bool`.
- `pty::shell_command(cwd, env: &[(String, String)])` sets extra env vars. The workspace passes `FUXX_SOCKET`, `FUXX_SESSION=s<N>`, and `PATH=<current PATH>:<exe dir>` only when scripting is on.
- `Session` gains `pub id: u64` from a `next_id` counter starting at 1 (`s1`, `s2`, …; never reused).

- [ ] **Step 1: Failing tests** (`grid.rs`, building a `Term` the way the existing grid tests do):

```rust
#[test]
fn text_lines_screen_last_and_all() {
    let mut t = term(20, 3); // existing helper style: 20 cols x 3 rows
    feed(&mut t, "one\r\ntwo\r\nthree\r\nfour"); // "one" scrolls into history
    assert_eq!(text_lines(&t, TextScope::Screen), "two\nthree\nfour");
    assert_eq!(text_lines(&t, TextScope::Last(2)), "three\nfour");
    assert_eq!(text_lines(&t, TextScope::All), "one\ntwo\nthree\nfour");
}

#[test]
fn text_lines_joins_wrapped_rows() {
    let mut t = term(5, 3);
    feed(&mut t, "abcdefgh");
    assert_eq!(text_lines(&t, TextScope::Screen), "abcdefgh");
}
```

(If the grid test module has no `term`/`feed` helpers, add minimal ones there.)

- [ ] **Step 2: Run** `cargo test grid`. Expected: compile errors.
- [ ] **Step 3: Implement** the interfaces.
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (169), 0 warnings.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m9): session ids, scripting env in shells, read and type helpers"`

### Task 5: Handling requests in the workspace — ✅

**Files:** Modify `src/workspace.rs` (or a new `src/workspace_api.rs` with `impl Workspace` if workspace.rs would pass ~1300 lines), `src/main.rs`.

**Interfaces:**
- `Workspace` takes the `Incoming` receiver (`Option<…>`) and drains it on the UI thread (`cx.spawn` loop, as terminal events do). Each request is answered with `conn.send_line(api::reply(id, result))`. Targets are resolved with `api::resolve` against `SessionRef { id, label }` in sidebar order, with `caller` from params and `focused` = the active session.
- Methods (spec semantics; result shapes in JSON):
  - `system.ping` → `{pong: true}`. `system.version` → `{version, methods}`.
  - `session.list` → `[{id: "s4", number, name, title, status, project, preset, focused, on_screen, note, progress}]`.
  - `session.new`:
    - Builds a `Launch` like ⌘T / ⌘O / presets. `folder` → project; `preset` by name (unknown → `not_found`); `run` → typed like a preset command; `name`.
    - `split` with `of` (default: the caller or the focused session) uses M8's split path.
    - `background` restores focus to the previously active session afterwards.
    - Returns `{id}`.
  - `session.close` / `session.focus` / `session.rename` (empty → automatic) → `{}`.
  - `session.send` → type `text` (+ `\r` if `enter`).
  - `session.key` → each name through `api::key_bytes(name, app_cursor)`; any unknown name → `bad_request`, nothing typed.
  - `session.paste` → `TerminalView::paste`.
  - `session.read` → `{text}` (`Screen` / `Last(n)` / `All`).
  - `session.status` → `{status}`.
  - `session.question` → `{text}` with the last 12 non-blank screen lines when status is Waiting, else `refused`.
  - `session.answer` → only when Waiting, or with `force`: type `text` + `\r`, else `refused`.
  - `preset.list` → `[{name, command}]`. `pane.equalize` → M8's equalize on the target's layout.
  - `notify` → `notifier::fire` on a background task.
  - Unknown method → `unknown_method`.
- Scripting changes go through the same functions as the keyboard and mouse paths, so saving, sidebar and layout rules apply unchanged.

- [ ] **Step 1:** Implement. No new unit tests: the pieces are Tasks 1–4's, and this is GPUI wiring.
- [ ] **Step 2: Run** `cargo build`, `cargo test`. Expected: 0 warnings, all pass. Remove the Task 1–3 dead-code allows that are now used.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m9): the workspace answers scripting requests"`

### Task 6: Events and wait — ✅

**Files:** Modify `src/workspace.rs` (or `workspace_api.rs`).

**Interfaces:**
- `events.subscribe`: replies `{subscribed: true}`, then keeps the `Conn` in `subscribers`. Every event is sent to each subscriber; dead ones are dropped on a failed `send_line`. The events are:
  - `session.opened` and `session.closed`;
  - `status.changed` (`from`, `to`);
  - `title.changed`, `session.renamed` and `focus.changed`;
  - `notification` (`title`, `body`).

  Each carries `session: {id, number, name}` where it applies, plus `ts` (unix millis).
- `session.wait`: params `target` or `any`, `for`, `text` and `timeout_ms`, parsed with `api::parse_wait`.
  - If already met, reply at once with `{id}`. Otherwise store `{conn, request id, session (or any), cond, start status, deadline}`.
  - Check stored waits on every status change, every exit (the session closed → `Exit` / `Change` met), and on a 200 ms timer while any wait is pending. Text waits read `TerminalView::text(Last(200))`.
  - Met → reply `{id: "s<N>"}`. Deadline passed → `timeout`. Dead conn → drop.
  - The timer runs only while waits exist.

- [ ] **Step 1:** Implement.
- [ ] **Step 2: Run** `cargo build`, `cargo test`. Expected: 0 warnings, all pass.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m9): event stream and wait"`

### Task 7: Note and progress in the sidebar — ✅

**Files:** Modify `src/workspace.rs`.

**Interfaces:**
- `Session` gains `note: Option<String>` and `progress: Option<(u8, Option<String>)>`. They're not saved for restore, since they belong to a running program.
- `session.note` takes `text`, or `null` to clear; empty also clears; capped at 120 chars. `session.progress` takes `percent` 0–100 (clamped), `label?`, or `null` to clear.
- The session row shows the note as a second line under the name: `text_xs`, the dim text colour, one line, cut with "…". Progress shows as a 2 px bar along the bottom of the row, in the ring-working colour, with its width set to the percent. The label is shown before the note when present.

- [ ] **Step 1:** Implement.
- [ ] **Step 2: Run** `cargo build`, `cargo test`. Expected: 0 warnings, all pass.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m9): session note and progress in the sidebar"`

### Task 8: `fuxx mcp` — ✅

**Files:** Create `src/mcp.rs` · Modify `src/main.rs`.

**Interfaces:**
- `pub fn handle(msg: &serde_json::Value, call: &mut dyn FnMut(Request) -> Result<serde_json::Value, ApiError>) -> Option<serde_json::Value>`:
  - `initialize` → `{protocolVersion: "2025-11-25", capabilities: {tools: {}}, serverInfo: {name: "fuxx", version}, instructions}`.
  - Notifications (no `id`) → `None`.
  - `ping` → `{}`.
  - `tools/list` → the spec's 14 tools, each with `name`, `description` and an `inputSchema` (JSON Schema object).
  - `tools/call` → maps `name` + `arguments` to a `Request`, calls it, and returns `{content: [{type: "text", text}], isError}`. The text is the CLI text rendering of the result, or the error message with `isError: true`.
  - Unknown method → JSON-RPC error `-32601`.
- `pub fn run() -> i32`: stdio loop. One JSON message per line in, one response line out. Calls go through `api_client::call`, and `wait` through `api_client::stream`, reading the single result line. stderr is used for logs only.

- [ ] **Step 1: Failing tests:**

```rust
fn rpc(id: u64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

#[test]
fn initialize_and_list_tools() {
    let mut never = |_: Request| -> Result<serde_json::Value, ApiError> { panic!("no call") };
    let r = handle(&rpc(1, "initialize", serde_json::json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}})), &mut never).unwrap();
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
    assert!(r["result"]["capabilities"]["tools"].is_object());
    assert!(handle(&serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}), &mut never).is_none());
    let tools = handle(&rpc(2, "tools/list", serde_json::json!({})), &mut never).unwrap();
    let names: Vec<_> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names.len(), 14);
    assert!(names.contains(&"answer".to_string()) && names.contains(&"wait".to_string()));
}

#[test]
fn tools_call_maps_to_requests_and_errors() {
    let mut seen = Vec::new();
    let mut fake = |req: Request| -> Result<serde_json::Value, ApiError> {
        seen.push(req.clone());
        if req.method == "session.answer" { Err(ApiError::refused("s4 is not waiting")) } else { Ok(serde_json::json!({"status": "waiting"})) }
    };
    let ok = handle(&rpc(3, "tools/call", serde_json::json!({"name": "status", "arguments": {"target": "s4"}})), &mut fake).unwrap();
    assert_eq!(ok["result"]["isError"], false);
    assert_eq!(ok["result"]["content"][0]["text"], "waiting");
    let bad = handle(&rpc(4, "tools/call", serde_json::json!({"name": "answer", "arguments": {"target": "s4", "text": "y"}})), &mut fake).unwrap();
    assert_eq!(bad["result"]["isError"], true);
    assert!(bad["result"]["content"][0]["text"].as_str().unwrap().contains("not waiting"));
    assert_eq!(seen[0].method, "session.status");
    assert_eq!(seen[0].params["target"], "s4");
    let unknown = handle(&rpc(5, "nope", serde_json::json!({})), &mut fake).unwrap();
    assert_eq!(unknown["error"]["code"], -32601);
}
```

- [ ] **Step 2: Run** `cargo test mcp`. Expected: compile errors.
- [ ] **Step 3: Implement** the interfaces.
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (171), 0 warnings.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m9): fuxx as an MCP server"`

### Task 9: On screen — ✅

Run `cargo run`, then in a fuxx session:

- [x] `fuxx ping` → ok. `fuxx ls` lists the sessions with ids. `echo $FUXX_SESSION $FUXX_SOCKET` shows them.
- [x] `fuxx new --preset claude --split right --background` → prints an id. A split appears, claude starts, and your focus stays.
- [x] `fuxx wait <id>` returns when Claude asks something. `fuxx question <id>` shows it. `fuxx answer <id> "…"` answers it. `fuxx answer` on a working session refuses.
- [x] `fuxx send <id> "echo hi" --enter`, `fuxx key <id> ctrl-c`, `echo hello | fuxx paste <id>`, `fuxx read <id> --lines 5`.
- [x] `fuxx events` in another pane prints status changes live. `fuxx wait --any --for done --timeout 5` exits 124 if nothing finishes.
- [x] `fuxx note "building"` and `fuxx progress 40 --label tests` show under the session's row; `--clear` removes them.
- [x] `fuxx rename api`, `fuxx focus api`, `fuxx equalize`, `fuxx close <id>`, `fuxx notify "done"`.
- [x] `claude mcp add fuxx -- fuxx mcp`: in Claude Code, "list my fuxx sessions" works, and reading one works.
- [x] `scripting = off` + restart → `fuxx ping` exits 3 with a clear message.
- [x] Everything from M5–M8 unchanged.

### Task 10: Update the docs — ✅

- [x] Milestone note 49 ✅ with the checklist; this plan ✅; roadmap M9 ✅; tracker; Progress Log.
- [x] `STRUCTURE.md`: `api.rs`, `api_server.rs`, `api_client.rs`, `cli.rs`, `mcp.rs`.
- [x] A short user doc: `docs/05-Agentic-Workflow/` or README section "Scripting fuxx" with the command table and the MCP setup line.

## Verification log

_(filled in as tasks complete)_

## Known limitations

**Calls made during the build (2026-10-27; you skipped the design questions):**
- Scripting is on by default; same-user socket in `~/Library/Application Support/fuxx/`.
- A live socket (connect succeeds) is never replaced; a second fuxx runs without scripting. Unknown commands (`fuxx lss`) are errors and never open a window.
- Shells get `FUXX_SOCKET`/`FUXX_SESSION` only when this fuxx is actually serving; otherwise inherited ones are removed.
- A caller session that's gone is an error, never the focused session. `answer` refuses unless the agent is waiting (`--force` overrides).
- Scripted typing counts as typing for the status ring. `wait --text` counts only text that appears after the wait starts.
- `fuxx focus` brings the window to the front. `fuxx ls` keeps the id as the first word (`s4  #2*  …`).
- MCP: waits default to 600 s, capped at 6 h; `fuxx mcp` exits within ~2 s when its client closes.

**Small issues left for later:**
- `wait --text`: >200 identical lines in one 0.2 s tick can hide a new marker; full-screen redraws of the top line can count old text again.
- 64 idle connections can hold every slot (no read timeout). A dead event subscriber holds its slot until the next event.
- A scripted `new`/`focus` cancels an open rename box; `--background` emits two `focus.changed` events; `title.changed` can be chatty.
- After the last session closes, the window can stay up to 0.5 s while replies flush.
- MCP `list_sessions` uses the CLI `ls` line; relative folders aren't made absolute by MCP (the app rejects them).

- `wait --text` is plain text, not a pattern.
- Ids last one launch.
- Text-wait polling is 200 ms.

## Related

- [[49 - Milestone 9 - Scripting API]] · [[40 - Roadmap]] · [[00 - Plan Tracker (Start Here)]]
