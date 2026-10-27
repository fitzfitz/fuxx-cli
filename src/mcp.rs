//! `fuxx mcp`: a Model Context Protocol server on stdio that forwards to the running fuxx, so an
//! agent in one session can list, start, read, wait on and answer agents in other sessions.
//!
//! Newline-delimited JSON-RPC 2.0: one message per line in, one reply line out. Only MCP
//! messages go to stdout; logs go to stderr. Each tool is one protocol request, and its text is
//! what the matching `fuxx` command prints, so the CLI and MCP read the same.

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::api::{ApiError, Request};
use crate::api_client::{self, ClientError};
use crate::cli;

/// The MCP versions this server speaks, newest first.
const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18"];

/// How long `wait` blocks when the agent gives no timeout, so a tool call can't hang forever.
const DEFAULT_WAIT_SECS: u64 = 600;
/// The longest `wait` an agent can ask for.
const MAX_WAIT_SECS: u64 = 6 * 60 * 60;

/// How long, at the end of input, calls still running get to finish and answer.
const EXIT_GRACE: Duration = Duration::from_secs(2);

/// JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

const INSTRUCTIONS: &str = "fuxx is the terminal app this server drives. Each fuxx session is a \
shell, often running an AI coding agent, and fuxx knows whether each one is idle, working, \
waiting for input or done. A typical flow: list_sessions to see what's open, new_session with a \
preset to start an agent, wait (for: waiting) until it needs input, question to read what it's \
asking, then answer it. A target is a session id like s4, #N for the Nth sidebar row, or a name; \
leaving it out means the session this server runs in.";

/// One tool: the protocol method it calls, and the `fuxx` command whose text it shows.
struct Tool {
    name: &'static str,
    method: &'static str,
    command: &'static str,
}

const TOOLS: &[Tool] = &[
    Tool {
        name: "list_sessions",
        method: "session.list",
        command: "ls",
    },
    Tool {
        name: "new_session",
        method: "session.new",
        command: "new",
    },
    Tool {
        name: "send",
        method: "session.send",
        command: "send",
    },
    Tool {
        name: "press_keys",
        method: "session.key",
        command: "key",
    },
    Tool {
        name: "read_screen",
        method: "session.read",
        command: "read",
    },
    Tool {
        name: "status",
        method: "session.status",
        command: "status",
    },
    Tool {
        name: "question",
        method: "session.question",
        command: "question",
    },
    Tool {
        name: "answer",
        method: "session.answer",
        command: "answer",
    },
    Tool {
        name: "wait",
        method: "session.wait",
        command: "wait",
    },
    Tool {
        name: "close_session",
        method: "session.close",
        command: "close",
    },
    Tool {
        name: "focus",
        method: "session.focus",
        command: "focus",
    },
    Tool {
        name: "rename",
        method: "session.rename",
        command: "rename",
    },
    Tool {
        name: "note",
        method: "session.note",
        command: "note",
    },
    Tool {
        name: "notify",
        method: "notify",
        command: "notify",
    },
];

fn target() -> Value {
    json!({
        "type": "string",
        "description": "Session id like s4, #N for the Nth sidebar row, or a name. Omit for the \
            session this MCP server runs in (outside fuxx: the focused session).",
    })
}

fn text(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn switch(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

/// A tool as `tools/list` shows it.
fn schema(name: &str) -> Value {
    let (description, properties, required): (&str, Value, &[&str]) = match name {
        "list_sessions" => (
            "List every fuxx session in sidebar order: id, #number, status (idle, working, \
             waiting or done), name, folder, agent preset, and progress and note when set. \
             A leading * marks the focused session, + another one on screen.",
            json!({}),
            &[],
        ),
        "new_session" => (
            "Open a new session (a shell), optionally launching an agent preset or typing a \
             command into it. Returns the new session's id.",
            json!({
                "folder": text("Absolute path of the folder to start in. Default: the folder \
                    of the focused session."),
                "preset": text("Name of an agent preset configured in fuxx (e.g. claude) to \
                    launch in the new session."),
                "run": text("A command to type into the new shell, the way a preset does."),
                "name": text("A name for the session, to find it again by name."),
                "split": {
                    "type": "string",
                    "enum": ["right", "down"],
                    "description": "Open it as a split pane beside a session. It is still a \
                        new session, with its own sidebar row.",
                },
                "of": text("With split: the session to split (id, #N or name). Default: the \
                    session this MCP server runs in."),
                "background": switch("Keep focus where it is instead of showing the new \
                    session."),
            }),
            &[],
        ),
        "send" => (
            "Type text into a session as if typed on the keyboard. Set enter to press Enter \
             after it. To reply to an agent that is waiting, prefer answer. Give a target; \
             omit target only to act on your own session.",
            json!({
                "target": target(),
                "text": text("The text to type."),
                "enter": switch("Press Enter after the text."),
            }),
            &["text"],
        ),
        "press_keys" => (
            "Press keys in a session, in order. Give a target; omit target only to act on your \
             own session.",
            json!({
                "target": target(),
                "keys": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Key names: enter, tab, esc, space, backspace, delete, up, \
                        down, left, right, home, end, pageup, pagedown, f1-f12, a single \
                        character, or ctrl-/alt-/shift- combinations like ctrl-c.",
                },
            }),
            &["keys"],
        ),
        "read_screen" => (
            "Read a session's terminal text: the visible screen by default, or the last N lines, \
             or all of its scrollback.",
            json!({
                "target": target(),
                "lines": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Only the last N lines of output (scrollback included).",
                },
                "all": switch("All the text, scrollback included."),
            }),
            &[],
        ),
        "status" => (
            "A session's agent status: idle, working, waiting (needs input) or done.",
            json!({ "target": target() }),
            &[],
        ),
        "question" => (
            "What a waiting session is asking: the last lines of its screen. Fails if the \
             session isn't waiting.",
            json!({ "target": target() }),
            &[],
        ),
        "answer" => (
            "Answer a waiting session: types the text and presses Enter. Refused unless the \
             session is waiting, so it never types into a busy agent. Give a target; omit \
             target only to act on your own session.",
            json!({
                "target": target(),
                "text": text("The answer, e.g. y, 1, or a sentence."),
                "force": switch("Answer even if the session isn't waiting."),
            }),
            &["text"],
        ),
        "wait" => (
            "Block until a session (or any session) meets a condition: reaches a status \
             (default: waiting, meaning it needs input), changes status, closes, or new text \
             appears on its screen. Returns the id of the session that met it; fails if the \
             timeout passes first. Waiting for a status the session already has (or, with any, \
             one that some session already has) returns immediately; use change to wait for \
             the next change. Give a target (another session) or any: true. \
             Omitting target waits on your own session, which is busy while you call this, so \
             it would only time out. Waits up to `timeout` seconds (default 600, max 21600). \
             If your MCP client gives up sooner, pass a shorter timeout and call again.",
            json!({
                "target": target(),
                "any": switch("Wait on every session instead of one; returns the first to meet \
                    the condition. Give no target with it."),
                "for": {
                    "type": "string",
                    "enum": ["waiting", "done", "idle", "working", "change", "exit"],
                    "description": "The condition (default waiting): a status, change (any \
                        status change) or exit (the session closed: its shell ended). A command \
                        finishing doesn't close the session; for that use done or text.",
                },
                "text": text("Wait until this plain text appears on the screen instead \
                    (overrides for). Only text that appears after the wait starts counts, but \
                    the command you typed is echoed on screen too: use a marker the command \
                    prints but doesn't contain, e.g. echo ALL''DONE (wait for ALLDONE) or \
                    echo $((1+1))DONE (wait for 2DONE)."),
                "timeout": {
                    "type": "number",
                    "minimum": 0,
                    "description": format!(
                        "Seconds to wait before giving up (default {DEFAULT_WAIT_SECS}, max \
                         {MAX_WAIT_SECS})."
                    ),
                },
            }),
            &[],
        ),
        "close_session" => (
            "Close a session; the program running in it is stopped. Give a target; omit target \
             only to act on your own session.",
            json!({ "target": target() }),
            &[],
        ),
        "focus" => (
            "Show and focus a session in the fuxx window, bringing fuxx to the front. Give a \
             target; omit target only to act on your own session.",
            json!({ "target": target() }),
            &[],
        ),
        "rename" => (
            "Rename a session. An empty name restores the automatic one. Give a target; omit \
             target only to act on your own session.",
            json!({ "target": target(), "name": text("The new name.") }),
            &["name"],
        ),
        "note" => (
            "Set the short note line under a session's name in the sidebar, e.g. what its \
             agent is working on. Empty text clears it.",
            json!({ "target": target(), "text": text("The note (empty clears it).") }),
            &["text"],
        ),
        "notify" => (
            "Post a macOS notification from fuxx.",
            json!({
                "title": text("The title (default: fuxx)."),
                "body": text("The message."),
            }),
            &["body"],
        ),
        other => unreachable!("no tool {other}"),
    };
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    })
}

/// The protocol request for a tool's `arguments`. Only the arguments its schema lists are
/// passed on; `wait`'s `timeout` (seconds) becomes `timeout_ms`, with a default.
fn request_for(tool: &Tool, args: Option<&Value>) -> Result<Request, String> {
    let empty = Map::new();
    let args = match args {
        None | Some(Value::Null) => &empty,
        Some(Value::Object(args)) => args,
        Some(_) => return Err("arguments must be an object".into()),
    };
    let mut params = Map::new();
    if let Some(properties) = schema(tool.name)["inputSchema"]["properties"].as_object() {
        for key in properties.keys() {
            if let Some(v) = args.get(key) {
                params.insert(key.clone(), v.clone());
            }
        }
    }
    if tool.name == "wait" {
        let secs = match params.remove("timeout") {
            None | Some(Value::Null) => DEFAULT_WAIT_SECS as f64,
            Some(v) => v
                .as_f64()
                .filter(|s| s.is_finite() && *s >= 0.0)
                .ok_or("timeout must be a number of seconds")?,
        }
        .min(MAX_WAIT_SECS as f64);
        params.insert("timeout_ms".into(), json!((secs * 1000.0).round() as u64));
    }
    Ok(Request {
        id: None,
        method: tool.method.to_string(),
        params: Value::Object(params),
    })
}

fn tool_result(text: String, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

/// `tools/call`: run the tool. A failure the agent can act on (bad arguments, a refusal, fuxx
/// not running) is a tool result with `isError`; only an unknown tool is a protocol error.
fn call_tool(
    params: &Value,
    call: &mut dyn FnMut(Request) -> Result<Value, ApiError>,
) -> Result<Value, (i64, String)> {
    let Some(name) = params["name"].as_str() else {
        return Err((INVALID_PARAMS, "tools/call needs a tool name".into()));
    };
    let Some(tool) = TOOLS.iter().find(|t| t.name == name) else {
        return Err((INVALID_PARAMS, format!("unknown tool: {name}")));
    };
    let reply = request_for(tool, params.get("arguments"))
        .and_then(|request| call(request).map_err(|e| e.message));
    Ok(match reply {
        Ok(result) => {
            let text = cli::render(tool.command, &result, false);
            // Commands that print nothing on success still tell the agent it worked.
            let done = result.is_null() || result.as_object().is_some_and(Map::is_empty);
            if text.is_empty() && done {
                tool_result("ok".into(), false)
            } else {
                tool_result(text, false)
            }
        }
        Err(message) => tool_result(message, true),
    })
}

fn initialize(params: &Value) -> Value {
    let asked = params["protocolVersion"].as_str();
    let version = asked
        .filter(|v| PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "fuxx", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn error_reply(id: Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The reply to one message, or `None` for a notification (no `id`), which gets none.
/// `call` sends a protocol request to fuxx.
pub fn handle(
    msg: &Value,
    call: &mut dyn FnMut(Request) -> Result<Value, ApiError>,
) -> Option<Value> {
    // One object per message; a batch (an array) isn't part of MCP.
    if !msg.is_object() {
        return Some(error_reply(
            Value::Null,
            INVALID_REQUEST,
            "invalid request".into(),
        ));
    }
    let id = msg.get("id")?.clone();
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        // A client's reply to a request: this server never sends any, so there's nothing to do.
        if msg.get("result").is_some() || msg.get("error").is_some() {
            return None;
        }
        return Some(error_reply(id, INVALID_REQUEST, "invalid request".into()));
    };
    let params = msg.get("params").unwrap_or(&Value::Null);
    let result = match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => {
            Ok(json!({ "tools": TOOLS.iter().map(|t| schema(t.name)).collect::<Vec<_>>() }))
        }
        "tools/call" => call_tool(params, call),
        other => Err((METHOD_NOT_FOUND, format!("method not found: {other}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_reply(id, code, message),
    })
}

/// One input line as JSON, or the parse-error reply (with a null id) to send instead.
fn parse_line(line: &str) -> Result<Value, Value> {
    serde_json::from_str(line)
        .map_err(|e| error_reply(Value::Null, PARSE_ERROR, format!("parse error: {e}")))
}

/// Mark a request as coming from `caller` (the session in `FUXX_SESSION`), so no target means
/// that session.
fn add_caller(request: &mut Request, caller: Option<u64>) {
    if let Some(caller) = caller {
        request.params["caller"] = json!(caller);
    }
}

/// `wait`'s reply: the one result or error line fuxx sends when the wait is over.
fn wait_reply(path: &Path, request: &Request) -> Result<Value, ClientError> {
    let mut reply = None;
    api_client::stream(path, request, |line| {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return true;
        };
        if value.get("result").is_none() && value.get("error").is_none() {
            return true;
        }
        reply = Some(api_client::parse_reply(line));
        false
    })?;
    reply.unwrap_or_else(|| {
        Err(ClientError::Unreachable(
            "fuxx closed the connection without replying".into(),
        ))
    })
}

/// Send one request to the running fuxx. Only an error's message reaches the agent.
fn forward(
    socket: Option<&Path>,
    caller: Option<u64>,
    mut request: Request,
) -> Result<Value, ApiError> {
    let Some(path) = socket else {
        return Err(ApiError::unavailable(
            "$HOME and FUXX_SOCKET are unset, so the fuxx socket can't be found",
        ));
    };
    add_caller(&mut request, caller);
    let reply = if request.method == "session.wait" {
        wait_reply(path, &request)
    } else {
        api_client::call(path, &request)
    };
    reply.map_err(|e| match e {
        ClientError::Unreachable(message) => ApiError::unavailable(message),
        ClientError::Api { message, .. } => ApiError::refused(message),
    })
}

/// Write one reply line. The lock keeps lines from concurrent tool calls whole.
fn send(reply: &Value) {
    let mut out = std::io::stdout().lock();
    // A closed stdout means the client is gone; the read loop ends on its own then.
    let _ = writeln!(out, "{reply}").and_then(|_| out.flush());
}

/// A lock that a panicked call thread can't poison for the rest.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Sends a protocol request to fuxx.
type Call = Arc<dyn Fn(Request) -> Result<Value, ApiError> + Send + Sync>;
/// Writes one reply line.
type Out = Arc<dyn Fn(&Value) + Send + Sync>;

/// The message loop without the stdio: input lines in, reply lines out through `out`.
struct Server {
    call: Call,
    out: Out,
    /// The ids (as JSON text) of tool calls still running whose replies are wanted; a
    /// cancelled call is taken out, so its reply is dropped.
    in_flight: Arc<Mutex<HashSet<String>>>,
    calls: Vec<JoinHandle<()>>,
}

impl Server {
    fn new(call: Call, out: Out) -> Self {
        Self {
            call,
            out,
            in_flight: Arc::default(),
            calls: Vec::new(),
        }
    }

    /// Handle one input line. Each tool call runs on its own thread, so a long `wait` doesn't
    /// hold up pings or other calls.
    fn line(&mut self, bytes: &[u8]) {
        let msg = match std::str::from_utf8(bytes).map_err(|e| e.to_string()) {
            Ok(line) if line.trim().is_empty() => return,
            Ok(line) => parse_line(line),
            Err(e) => Err(error_reply(
                Value::Null,
                PARSE_ERROR,
                format!("parse error: {e}"),
            )),
        };
        let msg = match msg {
            Ok(msg) => msg,
            Err(reply) => return (self.out)(&reply),
        };
        if msg["method"] == "notifications/cancelled" {
            // The call may still finish; only its reply is dropped.
            let id = msg["params"]["requestId"].to_string();
            lock(&self.in_flight).remove(&id);
            return;
        }
        if msg["method"] != "tools/call" {
            let mut call = |request: Request| (self.call)(request);
            if let Some(reply) = handle(&msg, &mut call) {
                (self.out)(&reply);
            }
            return;
        }
        let id = msg["id"].to_string();
        lock(&self.in_flight).insert(id.clone());
        let (call, out, in_flight) = (self.call.clone(), self.out.clone(), self.in_flight.clone());
        self.calls.retain(|c| !c.is_finished());
        self.calls.push(std::thread::spawn(move || {
            let mut call = |request: Request| call(request);
            let reply = handle(&msg, &mut call);
            if lock(&in_flight).remove(&id)
                && let Some(reply) = reply
            {
                out(&reply);
            }
        }));
    }

    /// At the end of input: calls still running get until `grace` to finish and answer. The
    /// rest (pending waits) are abandoned; their sockets close when the process exits, which
    /// ends them in fuxx.
    fn finish(mut self, grace: Duration) {
        let deadline = Instant::now() + grace;
        loop {
            self.calls.retain(|c| !c.is_finished());
            if self.calls.is_empty() || Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Serve MCP on stdio until standard input closes; returns the exit code. `socket` is the fuxx
/// socket, `caller` the session this runs in.
pub fn run(socket: Option<PathBuf>, caller: Option<u64>) -> i32 {
    match &socket {
        Some(path) => eprintln!("[fuxx] mcp: forwarding to {}", path.display()),
        None => eprintln!("[fuxx] mcp: $HOME and FUXX_SOCKET are unset; every tool will fail"),
    }
    let call: Call = Arc::new(move |request| forward(socket.as_deref(), caller, request));
    let mut server = Server::new(call, Arc::new(send));
    let mut stdin = std::io::stdin().lock();
    let mut buf = Vec::new();
    let code = loop {
        buf.clear();
        match stdin.read_until(b'\n', &mut buf) {
            Ok(0) => break 0,
            Ok(_) => server.line(&buf),
            Err(e) => {
                eprintln!("[fuxx] mcp: couldn't read standard input: {e}");
                break 1;
            }
        }
    };
    server.finish(EXIT_GRACE);
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{ApiError, Request};

    fn rpc(id: u64, method: &str, params: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }

    #[test]
    fn initialize_and_list_tools() {
        let mut never = |_: Request| -> Result<serde_json::Value, ApiError> { panic!("no call") };
        let r = handle(&rpc(1, "initialize", serde_json::json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}})), &mut never).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert!(
            handle(
                &serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
                &mut never
            )
            .is_none()
        );
        let tools = handle(&rpc(2, "tools/list", serde_json::json!({})), &mut never).unwrap();
        let names: Vec<_> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names.len(), 14);
        assert!(names.contains(&"answer".to_string()) && names.contains(&"wait".to_string()));
    }

    #[test]
    fn tools_call_maps_to_requests_and_errors() {
        let mut seen = Vec::new();
        let mut fake = |req: Request| -> Result<serde_json::Value, ApiError> {
            seen.push(req.clone());
            if req.method == "session.answer" {
                Err(ApiError::refused("s4 is not waiting"))
            } else {
                Ok(serde_json::json!({"status": "waiting"}))
            }
        };
        let ok = handle(
            &rpc(
                3,
                "tools/call",
                serde_json::json!({"name": "status", "arguments": {"target": "s4"}}),
            ),
            &mut fake,
        )
        .unwrap();
        assert_eq!(ok["result"]["isError"], false);
        assert_eq!(ok["result"]["content"][0]["text"], "waiting");
        let bad = handle(
            &rpc(
                4,
                "tools/call",
                serde_json::json!({"name": "answer", "arguments": {"target": "s4", "text": "y"}}),
            ),
            &mut fake,
        )
        .unwrap();
        assert_eq!(bad["result"]["isError"], true);
        assert!(
            bad["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("not waiting")
        );
        let unknown = handle(&rpc(5, "nope", serde_json::json!({})), &mut fake).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
        assert_eq!(seen[0].method, "session.status");
        assert_eq!(seen[0].params["target"], "s4");
    }

    #[test]
    fn invalid_json_is_a_parse_error_with_a_null_id() {
        let e = parse_line("{not json").unwrap_err();
        assert_eq!(e["error"]["code"], -32700);
        assert!(e["id"].is_null());
        assert_eq!(e["jsonrpc"], "2.0");
        assert!(!e.to_string().contains('\n'));
        assert_eq!(parse_line(r#"{"id": 1}"#).unwrap()["id"], 1);
        let mut never = |_: Request| -> Result<serde_json::Value, ApiError> { panic!("no call") };
        let batch = handle(
            &serde_json::json!([rpc(1, "ping", serde_json::json!({}))]),
            &mut never,
        );
        assert_eq!(batch.unwrap()["error"]["code"], -32600);
    }

    #[test]
    fn protocol_version_is_the_clients_when_supported_else_ours() {
        let mut never = |_: Request| -> Result<serde_json::Value, ApiError> { panic!("no call") };
        let version = |v: serde_json::Value, call: &mut dyn FnMut(Request) -> _| {
            let init = rpc(1, "initialize", serde_json::json!({"protocolVersion": v}));
            handle(&init, call).unwrap()["result"]["protocolVersion"].clone()
        };
        assert_eq!(version("2025-06-18".into(), &mut never), "2025-06-18");
        assert_eq!(version("2025-11-25".into(), &mut never), "2025-11-25");
        assert_eq!(version("1999-01-01".into(), &mut never), "2025-11-25");
        assert_eq!(version(serde_json::Value::Null, &mut never), "2025-11-25");
        let init = handle(&rpc(1, "initialize", serde_json::json!({})), &mut never).unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "fuxx");
        assert!(
            init["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("wait")
        );
        assert_eq!(init["jsonrpc"], "2.0");
        assert_eq!(init["id"], 1);
    }

    #[test]
    fn caller_is_added_as_a_number() {
        let mut req = Request {
            id: None,
            method: "session.status".into(),
            params: serde_json::json!({"target": "s4"}),
        };
        add_caller(&mut req, None);
        assert!(req.params.get("caller").is_none());
        add_caller(&mut req, Some(7));
        assert_eq!(req.params["caller"], serde_json::json!(7));
        assert!(req.params["caller"].is_u64());
        assert_eq!(req.params["target"], "s4");
    }

    #[test]
    fn wait_gets_a_default_timeout_and_seconds_become_ms() {
        let mut seen = Vec::new();
        let mut fake = |req: Request| -> Result<serde_json::Value, ApiError> {
            seen.push(req);
            Ok(serde_json::json!({"id": "s4"}))
        };
        let call = |args: serde_json::Value| {
            rpc(
                1,
                "tools/call",
                serde_json::json!({"name": "wait", "arguments": args}),
            )
        };
        let r = handle(&call(serde_json::json!({"target": "s4"})), &mut fake).unwrap();
        assert_eq!(r["result"]["content"][0]["text"], "s4");
        handle(
            &call(serde_json::json!({"for": "done", "timeout": 2.5})),
            &mut fake,
        )
        .unwrap();
        let bad = handle(&call(serde_json::json!({"timeout": "soon"})), &mut fake).unwrap();
        assert_eq!(bad["result"]["isError"], true);
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].method, "session.wait");
        assert_eq!(seen[0].params["timeout_ms"], DEFAULT_WAIT_SECS * 1000);
        assert_eq!(seen[1].params["timeout_ms"], 2500);
        assert_eq!(seen[1].params["for"], "done");
        assert!(seen[1].params.get("timeout").is_none());
    }

    #[test]
    fn arguments_outside_the_schema_are_dropped_and_empty_results_say_ok() {
        let mut seen = Vec::new();
        let mut fake = |req: Request| -> Result<serde_json::Value, ApiError> {
            seen.push(req);
            Ok(serde_json::json!({}))
        };
        let call = rpc(
            1,
            "tools/call",
            serde_json::json!({"name": "send", "arguments": {"text": "hi", "enter": true, "caller": 99, "bogus": 1}}),
        );
        let r = handle(&call, &mut fake).unwrap();
        assert_eq!(r["result"]["content"][0]["text"], "ok");
        assert_eq!(r["result"]["content"][0]["type"], "text");
        let unknown_tool = handle(
            &rpc(2, "tools/call", serde_json::json!({"name": "rm_rf"})),
            &mut fake,
        )
        .unwrap();
        assert_eq!(unknown_tool["error"]["code"], -32602);
        assert_eq!(seen[0].method, "session.send");
        assert_eq!(
            seen[0].params,
            serde_json::json!({"text": "hi", "enter": true})
        );
        assert_eq!(seen.len(), 1);
    }

    #[test]
    fn every_tool_maps_to_its_method() {
        let mut never = |_: Request| -> Result<serde_json::Value, ApiError> { panic!("no call") };
        let tools = handle(&rpc(1, "tools/list", serde_json::json!({})), &mut never).unwrap();
        for tool in tools["result"]["tools"].as_array().unwrap() {
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
            assert!(!tool["description"].as_str().unwrap().is_empty(), "{tool}");
        }
        let want = [
            ("list_sessions", "session.list"),
            ("new_session", "session.new"),
            ("send", "session.send"),
            ("press_keys", "session.key"),
            ("read_screen", "session.read"),
            ("status", "session.status"),
            ("question", "session.question"),
            ("answer", "session.answer"),
            ("wait", "session.wait"),
            ("close_session", "session.close"),
            ("focus", "session.focus"),
            ("rename", "session.rename"),
            ("note", "session.note"),
            ("notify", "notify"),
        ];
        for (name, method) in want {
            let mut got = None;
            let mut fake = |req: Request| -> Result<serde_json::Value, ApiError> {
                got = Some(req.method);
                Ok(serde_json::json!({}))
            };
            handle(
                &rpc(1, "tools/call", serde_json::json!({"name": name})),
                &mut fake,
            )
            .unwrap();
            assert_eq!(got.as_deref(), Some(method), "{name}");
        }
    }

    #[test]
    fn wait_timeouts_are_capped_and_must_be_numbers() {
        let mut seen = Vec::new();
        let mut fake = |req: Request| -> Result<serde_json::Value, ApiError> {
            seen.push(req);
            Ok(serde_json::json!({"id": "s4"}))
        };
        let call = |args: serde_json::Value| {
            rpc(
                1,
                "tools/call",
                serde_json::json!({"name": "wait", "arguments": args}),
            )
        };
        handle(
            &call(serde_json::json!({"any": true, "timeout": 99999})),
            &mut fake,
        )
        .unwrap();
        handle(
            &call(serde_json::json!({"any": true, "timeout": 21600})),
            &mut fake,
        )
        .unwrap();
        for bad in [
            serde_json::json!(-1),
            serde_json::json!("soon"),
            serde_json::json!(true),
        ] {
            let r = handle(&call(serde_json::json!({ "timeout": bad })), &mut fake).unwrap();
            assert_eq!(r["result"]["isError"], true, "{bad}");
        }
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].params["timeout_ms"], MAX_WAIT_SECS * 1000);
        assert_eq!(seen[1].params["timeout_ms"], 21_600_000);
    }

    #[test]
    fn tools_call_without_a_name_is_invalid_params() {
        let mut never = |_: Request| -> Result<serde_json::Value, ApiError> { panic!("no call") };
        let r = handle(&rpc(1, "tools/call", serde_json::json!({})), &mut never).unwrap();
        assert_eq!(r["error"]["code"], -32602);
        assert_eq!(r["error"]["message"], "tools/call needs a tool name");
    }

    #[test]
    fn descriptions_say_what_exit_split_and_text_mean() {
        let description = |name: &str| schema(name)["description"].as_str().unwrap().to_string();
        let wait = schema("wait");
        let props = &wait["inputSchema"]["properties"];
        let for_ = props["for"]["description"].as_str().unwrap();
        assert!(
            for_.contains("exit (the session closed: its shell ended)"),
            "{for_}"
        );
        let wait_text = description("wait");
        assert!(wait_text.contains("returns immediately"), "{wait_text}");
        assert!(!wait_text.contains("its program exits"), "{wait_text}");
        let text = props["text"]["description"].as_str().unwrap();
        assert!(text.contains("appears after the wait starts"), "{text}");
        assert!(text.contains("echo ALL''DONE"), "{text}");
        let split = schema("new_session")["inputSchema"]["properties"]["split"]["description"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(split.contains("its own sidebar row"), "{split}");
        assert!(!split.contains("instead of"), "{split}");
        let list = description("list_sessions");
        assert!(list.contains("* marks the focused session"), "{list}");
    }

    #[test]
    fn descriptions_warn_about_omitting_the_target() {
        let description = |name: &str| schema(name)["description"].as_str().unwrap().to_string();
        let wait = description("wait");
        assert!(wait.contains("any: true"), "{wait}");
        assert!(wait.contains("would only time out"), "{wait}");
        assert!(wait.contains("default 600, max 21600"), "{wait}");
        assert!(
            wait.contains("pass a shorter timeout and call again"),
            "{wait}"
        );
        for name in [
            "close_session",
            "focus",
            "rename",
            "send",
            "press_keys",
            "answer",
        ] {
            assert!(
                description(name).contains("omit target only to act on your own session"),
                "{name}"
            );
        }
    }

    type Lines = std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>;

    /// A server whose `session.wait` blocks until `release` gets a message (or is dropped) and
    /// whose other calls answer at once; its reply lines are collected.
    fn blocking_server() -> (Server, std::sync::mpsc::Sender<()>, Lines) {
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let blocked = std::sync::Mutex::new(blocked);
        let call: Call = std::sync::Arc::new(move |req: Request| {
            if req.method == "session.wait" {
                let _ = blocked.lock().unwrap().recv();
            }
            Ok(serde_json::json!({"status": "idle", "id": "s4"}))
        });
        let lines: Lines = Default::default();
        let sink = lines.clone();
        let out: Out =
            std::sync::Arc::new(move |v: &serde_json::Value| sink.lock().unwrap().push(v.clone()));
        (Server::new(call, out), release, lines)
    }

    fn tool_line(id: u64, name: &str) -> Vec<u8> {
        rpc(id, "tools/call", serde_json::json!({ "name": name }))
            .to_string()
            .into_bytes()
    }

    fn ids(lines: &Lines) -> Vec<serde_json::Value> {
        let mut ids: Vec<_> = lines
            .lock()
            .unwrap()
            .iter()
            .map(|l| l["id"].clone())
            .collect();
        ids.sort_by_key(|id| id.to_string());
        ids
    }

    #[test]
    fn a_cancelled_call_gets_no_reply() {
        let (mut server, release, lines) = blocking_server();
        server.line(&tool_line(7, "wait"));
        server.line(&tool_line(8, "wait"));
        server.line(br#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":7,"reason":"user"}}"#);
        release.send(()).unwrap();
        release.send(()).unwrap();
        server.finish(std::time::Duration::from_secs(5));
        assert_eq!(ids(&lines), vec![serde_json::json!(8)]);
    }

    #[test]
    fn end_of_input_answers_fast_calls_and_abandons_pending_waits() {
        let (mut server, _release, lines) = blocking_server();
        server.line(&tool_line(1, "wait"));
        server.line(&tool_line(3, "status"));
        server.line(
            &rpc(2, "ping", serde_json::json!({}))
                .to_string()
                .into_bytes(),
        );
        server.line(b"{oops");
        let start = std::time::Instant::now();
        server.finish(std::time::Duration::from_millis(300));
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(
            ids(&lines),
            vec![
                serde_json::json!(2),
                serde_json::json!(3),
                serde_json::Value::Null
            ]
        );
    }
}
