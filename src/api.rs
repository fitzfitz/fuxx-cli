//! The scripting protocol without any I/O: request/reply shapes, session targets, key names,
//! and wait conditions. The socket server, CLI and MCP server are built on this.
//!
//! One JSON object per line. Pure and GPUI-free, so every rule is unit-tested.

use serde::{Deserialize, Serialize};

use crate::keys::key_to_bytes;
use crate::status::Status;

/// Every method name the protocol accepts.
pub const METHODS: &[&str] = &[
    "system.ping",
    "system.version",
    "session.list",
    "session.new",
    "session.close",
    "session.focus",
    "session.rename",
    "session.send",
    "session.key",
    "session.paste",
    "session.read",
    "session.status",
    "session.question",
    "session.answer",
    "session.wait",
    "session.note",
    "session.progress",
    "pane.equalize",
    "preset.list",
    "notify",
    "events.subscribe",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    fn new(code: &'static str, msg: impl Into<String>) -> Self {
        Self {
            code,
            message: msg.into(),
        }
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new("bad_request", msg)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new("not_found", msg)
    }
    pub fn ambiguous(msg: impl Into<String>) -> Self {
        Self::new("ambiguous", msg)
    }
    pub fn refused(msg: impl Into<String>) -> Self {
        Self::new("refused", msg)
    }
    pub fn unknown_method(msg: impl Into<String>) -> Self {
        Self::new("unknown_method", msg)
    }
    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::new("timeout", msg)
    }
    pub fn unavailable(msg: impl Into<String>) -> Self {
        Self::new("unavailable", msg)
    }
}

/// One reply line (no trailing newline). `id` is `null` when the request had none.
pub fn reply(id: Option<u64>, result: Result<serde_json::Value, ApiError>) -> String {
    let id = serde_json::json!(id);
    let value = match result {
        Ok(result) => serde_json::json!({ "id": id, "result": result }),
        Err(error) => serde_json::json!({ "id": id, "error": error }),
    };
    value.to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// `s<digits>`: the session's stable id.
    Id(u64),
    /// `#<digits>`: its 1-based place in the sidebar.
    Number(usize),
    /// Anything else: its label, case-insensitively.
    Name(String),
}

pub fn parse_target(s: &str) -> Target {
    let digits = |rest: &str| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit());
    if let Some(rest) = s.strip_prefix('s') {
        if digits(rest) {
            if let Ok(id) = rest.parse() {
                return Target::Id(id);
            }
        }
    }
    if let Some(rest) = s.strip_prefix('#') {
        if digits(rest) {
            if let Ok(n) = rest.parse() {
                return Target::Number(n);
            }
        }
    }
    Target::Name(s.to_string())
}

/// A session as the resolver sees it, in sidebar order.
#[derive(Debug, Clone)]
pub struct SessionRef {
    pub id: u64,
    pub label: String,
}

/// Pick the session a command is about. No target means the calling session (if it still
/// exists), else the focused one.
pub fn resolve(
    target: Option<&Target>,
    sessions: &[SessionRef],
    caller: Option<u64>,
    focused: Option<u64>,
) -> Result<u64, ApiError> {
    let exists = |id: u64| sessions.iter().any(|s| s.id == id);
    match target {
        None => caller
            .filter(|&id| exists(id))
            .or(focused.filter(|&id| exists(id)))
            .ok_or_else(|| ApiError::not_found("no session to act on")),
        Some(Target::Id(id)) => {
            if exists(*id) {
                Ok(*id)
            } else {
                Err(ApiError::not_found(format!("no session s{id}")))
            }
        }
        Some(Target::Number(n)) => n
            .checked_sub(1)
            .and_then(|i| sessions.get(i))
            .map(|s| s.id)
            .ok_or_else(|| ApiError::not_found(format!("no session #{n}"))),
        Some(Target::Name(name)) => {
            let want = name.to_lowercase();
            let hits: Vec<u64> = sessions
                .iter()
                .filter(|s| s.label.to_lowercase() == want)
                .map(|s| s.id)
                .collect();
            match hits.as_slice() {
                [] => Err(ApiError::not_found(format!("no session named \"{name}\""))),
                [id] => Ok(*id),
                many => {
                    let ids: Vec<String> = many.iter().map(|id| format!("s{id}")).collect();
                    Err(ApiError::ambiguous(format!(
                        "\"{name}\" matches {}",
                        ids.join(", ")
                    )))
                }
            }
        }
    }
}

/// Bytes for a key name like `enter`, `ctrl-c`, `alt-left`, `shift-tab`, `f5`.
pub fn key_bytes(name: &str, app_cursor: bool) -> Option<Vec<u8>> {
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    let mut rest = name;
    loop {
        if let Some(r) = rest.strip_prefix("ctrl-") {
            ctrl = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("alt-") {
            alt = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("shift-") {
            shift = true;
            rest = r;
        } else {
            break;
        }
    }
    let mut chars = rest.chars();
    let (key, key_char) = match (chars.next(), chars.next()) {
        (Some(c), None) => {
            let shown = if shift {
                c.to_uppercase().to_string()
            } else {
                c.to_string()
            };
            (c.to_lowercase().to_string(), Some(shown))
        }
        _ => {
            let lower = rest.to_lowercase();
            let key = if lower == "esc" {
                "escape".to_string()
            } else {
                lower
            };
            let key_char = (key == "space").then(|| " ".to_string());
            (key, key_char)
        }
    };
    key_to_bytes(&key, key_char.as_deref(), ctrl, alt, shift, app_cursor)
}

#[derive(Debug, Clone, PartialEq)]
pub enum WaitFor {
    Status(Status),
    Change,
    Exit,
    Text(String),
}

/// `text` wins; otherwise `for_` (default `waiting`): `change`, `exit`, or a status name.
pub fn parse_wait(for_: Option<&str>, text: Option<&str>) -> Result<WaitFor, ApiError> {
    if let Some(t) = text {
        return Ok(WaitFor::Text(t.to_string()));
    }
    match for_.unwrap_or("waiting") {
        "change" => Ok(WaitFor::Change),
        "exit" => Ok(WaitFor::Exit),
        other => Status::parse(other).map(WaitFor::Status).ok_or_else(|| {
            ApiError::bad_request(format!(
                "unknown wait condition \"{other}\" (idle, working, waiting, done, change, exit)"
            ))
        }),
    }
}

pub fn wait_met(cond: &WaitFor, start: Status, now: Status, screen: &str, exited: bool) -> bool {
    match cond {
        WaitFor::Exit => exited,
        WaitFor::Change => exited || now != start,
        WaitFor::Status(s) => now == *s,
        WaitFor::Text(t) => screen.contains(t.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs() -> Vec<SessionRef> {
        vec![
            SessionRef {
                id: 1,
                label: "🖕".into(),
            },
            SessionRef {
                id: 4,
                label: "api".into(),
            },
            SessionRef {
                id: 7,
                label: "🖕".into(),
            },
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
        assert_eq!(
            resolve(Some(&Target::Name("API".into())), &r, None, None),
            Ok(4)
        );
        assert_eq!(resolve(None, &r, Some(7), Some(1)), Ok(7));
        assert_eq!(resolve(None, &r, Some(99), Some(1)), Ok(1)); // caller gone: the focused one
    }

    #[test]
    fn resolve_errors() {
        let r = refs();
        assert_eq!(
            resolve(Some(&Target::Id(9)), &r, None, None)
                .unwrap_err()
                .code,
            "not_found"
        );
        assert_eq!(
            resolve(Some(&Target::Number(0)), &r, None, None)
                .unwrap_err()
                .code,
            "not_found"
        );
        let e = resolve(Some(&Target::Name("🖕".into())), &r, None, None).unwrap_err();
        assert_eq!(e.code, "ambiguous");
        assert!(e.message.contains("s1") && e.message.contains("s7"));
        assert_eq!(
            resolve(None, &[], None, None).unwrap_err().code,
            "not_found"
        );
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
        assert_eq!(
            parse_wait(Some("done"), Some("ok")),
            Ok(WaitFor::Text("ok".into()))
        );
        assert_eq!(
            parse_wait(Some("bogus"), None).unwrap_err().code,
            "bad_request"
        );
        assert!(wait_met(
            &WaitFor::Status(Status::Done),
            Status::Working,
            Status::Done,
            "",
            false
        ));
        assert!(!wait_met(
            &WaitFor::Change,
            Status::Working,
            Status::Working,
            "",
            false
        ));
        assert!(wait_met(
            &WaitFor::Change,
            Status::Working,
            Status::Working,
            "",
            true
        ));
        assert!(wait_met(
            &WaitFor::Text("PASS".into()),
            Status::Idle,
            Status::Idle,
            "tests PASS\n",
            false
        ));
    }

    #[test]
    fn reply_shapes() {
        assert_eq!(
            reply(Some(1), Ok(serde_json::json!({"pong": true}))),
            r#"{"id":1,"result":{"pong":true}}"#
        );
        assert_eq!(
            reply(None, Err(ApiError::not_found("no session s9"))),
            r#"{"id":null,"error":{"code":"not_found","message":"no session s9"}}"#
        );
    }
}
