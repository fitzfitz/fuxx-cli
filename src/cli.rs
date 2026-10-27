//! The `fuxx <command>` command line, without any I/O: arguments → protocol request, and a
//! reply → the text a person (or `--json`) sees. The socket client in `main.rs` does the I/O.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value, json};

use crate::api::{Request, key_bytes};

/// What the command line asks for.
#[derive(Debug)]
pub enum Cli {
    /// Open the app window.
    Gui,
    /// Run as an MCP server on stdio.
    Mcp,
    /// Print this usage text.
    Help(String),
    /// Send one request. `stream`: read lines until the reply line or the end (`events`, `wait`).
    Call {
        /// The command word (`ls`, `wait`, …), for `render`.
        command: &'static str,
        request: Request,
        json: bool,
        stream: bool,
    },
}

const USAGE: &str = "\
usage: fuxx [command]   (no command opens the app)

  ls                                   list sessions
  new [--folder DIR] [--preset NAME] [--run CMD] [--name NAME]
      [--split right|down [--of TARGET]] [--background]   open a session; prints its id
  close [TARGET]                       close a session
  focus [TARGET]                       show and focus a session
  rename [TARGET] NAME                 rename (\"\" restores the automatic name)
  send [TARGET] TEXT [--enter]         type text, optionally press Enter
  key [TARGET] KEY...                  press keys (enter, ctrl-c, up, f5, ...)
  paste [TARGET]                       paste standard input as one block
  read [TARGET] [--lines N] [--all]    print the screen text
  status [TARGET]                      print idle, working, waiting or done
  question [TARGET]                    print what a waiting session is asking
  answer [TARGET] TEXT [--force]       answer a waiting session
  wait [TARGET | --any] [--for waiting|done|idle|working|change|exit]
       [--text TEXT] [--timeout SECS]  block until the condition holds
  events                               print one line per event until stopped
  note [TARGET] TEXT | --clear         a short line under the session's name
  progress [TARGET] 0-100 [--label TEXT] | --clear   a progress bar under the row
  notify [--title T] BODY              a macOS notification
  equalize [TARGET]                    even out the panes of a layout
  presets                              list agent presets
  ping                                 check that fuxx is reachable
  version                              print the version and the methods
  mcp                                  run as an MCP server on stdio
  help                                 this text

Every command takes --json. TARGET is s4, #2 or a session name.
Exit codes: 0 ok, 1 refused or nothing to show, 2 bad arguments, 3 fuxx not reachable, 124 wait timed out.
";

/// Command name → protocol method.
const COMMANDS: &[(&str, &str)] = &[
    ("ls", "session.list"),
    ("new", "session.new"),
    ("close", "session.close"),
    ("focus", "session.focus"),
    ("rename", "session.rename"),
    ("send", "session.send"),
    ("key", "session.key"),
    ("paste", "session.paste"),
    ("read", "session.read"),
    ("status", "session.status"),
    ("question", "session.question"),
    ("answer", "session.answer"),
    ("wait", "session.wait"),
    ("events", "events.subscribe"),
    ("note", "session.note"),
    ("progress", "session.progress"),
    ("notify", "notify"),
    ("equalize", "pane.equalize"),
    ("presets", "preset.list"),
    ("ping", "system.ping"),
    ("version", "system.version"),
];

/// The flags a command accepts: (takes a value, switches).
fn flag_spec(cmd: &str) -> (&'static [&'static str], &'static [&'static str]) {
    match cmd {
        "new" => (
            &["folder", "preset", "run", "name", "split", "of"],
            &["background"],
        ),
        "send" => (&[], &["enter"]),
        "read" => (&["lines"], &["all"]),
        "answer" => (&[], &["force"]),
        "wait" => (&["for", "text", "timeout"], &["any"]),
        "note" => (&[], &["clear"]),
        "progress" => (&["label"], &["clear"]),
        "notify" => (&["title"], &[]),
        _ => (&[], &[]),
    }
}

/// Arguments split into positionals, `--flag value` pairs and `--switch`es.
struct Args {
    pos: Vec<String>,
    vals: HashMap<String, String>,
    switches: HashSet<String>,
}

impl Args {
    fn split(cmd: &str, args: &[String]) -> Result<Args, String> {
        let (valued, switches) = flag_spec(cmd);
        let mut out = Args {
            pos: Vec::new(),
            vals: HashMap::new(),
            switches: HashSet::new(),
        };
        let mut it = args.iter();
        while let Some(a) = it.next() {
            if a == "--" {
                out.pos.extend(it.by_ref().cloned());
                break;
            }
            let Some(flag) = a.strip_prefix("--") else {
                out.pos.push(a.clone());
                continue;
            };
            let (name, inline) = match flag.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (flag, None),
            };
            if name == "json" || switches.contains(&name) {
                if inline.is_some() {
                    return Err(format!("--{name} takes no value"));
                }
                out.switches.insert(name.to_string());
            } else if valued.contains(&name) {
                let v = match inline {
                    Some(v) => v,
                    None => it
                        .next()
                        .cloned()
                        .ok_or_else(|| format!("--{name} needs a value"))?,
                };
                out.vals.insert(name.to_string(), v);
            } else {
                return Err(format!("unknown option --{name} for `fuxx {cmd}`"));
            }
        }
        Ok(out)
    }

    fn on(&self, name: &str) -> bool {
        self.switches.contains(name)
    }
    fn val(&self, name: &str) -> Option<&str> {
        self.vals.get(name).map(String::as_str)
    }
}

fn usage_err(cmd: &str, what: &str) -> String {
    format!("fuxx {cmd}: {what} (see `fuxx help`)")
}

/// Parse the command line (without argv[0]). `caller` is the session in `FUXX_SESSION`.
/// Only no arguments (or macOS's `-psn_…` launch argument) opens the app; anything else that
/// isn't a command is an error, so a typo never opens a window.
pub fn parse(args: &[String], caller: Option<u64>) -> Result<Cli, String> {
    // `--json` may also come before the command: it's moved to just after the command word.
    let leading_json = args.iter().take_while(|a| *a == "--json").count();
    let rest = &args[leading_json..];
    let Some(first) = rest.first() else {
        return if leading_json > 0 {
            Err("fuxx: --json needs a command (see `fuxx help`)".to_string())
        } else {
            Ok(Cli::Gui)
        };
    };
    if leading_json == 0 && first.starts_with("-psn_") {
        return Ok(Cli::Gui);
    }
    match first.as_str() {
        "help" | "--help" | "-h" => return Ok(Cli::Help(USAGE.to_string())),
        "mcp" => return Ok(Cli::Mcp),
        _ => {}
    }
    let Some(&(cmd, method)) = COMMANDS.iter().find(|(c, _)| *c == first.as_str()) else {
        return Err(format!("fuxx: unknown command: {first} (see `fuxx help`)"));
    };
    let mut tail: Vec<String> = Vec::with_capacity(rest.len());
    if leading_json > 0 {
        tail.push("--json".to_string());
    }
    tail.extend_from_slice(&rest[1..]);
    let a = Args::split(cmd, &tail)?;
    let mut params = Map::new();
    let pos = &a.pos;
    let n = pos.len();

    // `[TARGET] X`: how many positionals the command needs besides the optional target.
    let target_then =
        |want: usize, params: &mut Map<String, Value>| -> Result<Vec<String>, String> {
            if n == want {
                Ok(pos.clone())
            } else if n == want + 1 {
                params.insert("target".into(), json!(pos[0]));
                Ok(pos[1..].to_vec())
            } else {
                Err(usage_err(cmd, "wrong number of arguments"))
            }
        };

    match cmd {
        "ls" | "presets" | "ping" | "version" | "events" => {
            if n != 0 {
                return Err(usage_err(cmd, "takes no arguments"));
            }
        }
        "close" | "focus" | "status" | "question" | "equalize" | "paste" => {
            target_then(0, &mut params)?;
            if cmd == "paste" {
                params.insert("text".into(), Value::Null);
            }
        }
        "new" => {
            if n != 0 {
                return Err(usage_err(cmd, "takes no arguments"));
            }
            let split = a.val("split");
            if let Some(s) = split
                && s != "right"
                && s != "down"
            {
                return Err(usage_err(cmd, "--split must be right or down"));
            }
            if a.val("of").is_some() && split.is_none() {
                return Err(usage_err(cmd, "--of needs --split"));
            }
            for k in ["folder", "preset", "run", "name", "split", "of"] {
                params.insert(k.into(), a.val(k).map_or(Value::Null, |v| json!(v)));
            }
            params.insert("background".into(), json!(a.on("background")));
        }
        "rename" => {
            let rest = target_then(1, &mut params)?;
            params.insert("name".into(), json!(rest[0]));
        }
        "send" => {
            let rest = target_then(1, &mut params)?;
            params.insert("text".into(), json!(rest[0]));
            params.insert("enter".into(), json!(a.on("enter")));
        }
        "answer" => {
            let rest = target_then(1, &mut params)?;
            params.insert("text".into(), json!(rest[0]));
            params.insert("force".into(), json!(a.on("force")));
        }
        "key" => {
            if n == 0 {
                return Err(usage_err(cmd, "needs at least one key"));
            }
            let keys = if n >= 2 && key_bytes(&pos[0], false).is_none() {
                params.insert("target".into(), json!(pos[0]));
                &pos[1..]
            } else {
                &pos[..]
            };
            params.insert("keys".into(), json!(keys));
        }
        "read" => {
            target_then(0, &mut params)?;
            if let Some(l) = a.val("lines") {
                let l: u64 = l
                    .parse()
                    .map_err(|_| usage_err(cmd, "--lines needs a whole number"))?;
                params.insert("lines".into(), json!(l));
            }
            params.insert("all".into(), json!(a.on("all")));
        }
        "wait" => {
            if a.on("any") {
                if n != 0 {
                    return Err(usage_err(cmd, "--any takes no target"));
                }
            } else {
                target_then(0, &mut params)?;
            }
            params.insert("any".into(), json!(a.on("any")));
            for (flag, key) in [("for", "for"), ("text", "text")] {
                if let Some(v) = a.val(flag) {
                    params.insert(key.into(), json!(v));
                }
            }
            if let Some(t) = a.val("timeout") {
                let secs: f64 = t
                    .parse()
                    .ok()
                    .filter(|s: &f64| s.is_finite() && *s >= 0.0)
                    .ok_or_else(|| usage_err(cmd, "--timeout needs a number of seconds"))?;
                params.insert("timeout_ms".into(), json!((secs * 1000.0).round() as u64));
            }
        }
        "note" => {
            if a.on("clear") {
                target_then(0, &mut params)?;
                params.insert("text".into(), Value::Null);
            } else {
                let rest = target_then(1, &mut params)?;
                params.insert("text".into(), json!(rest[0]));
            }
        }
        "progress" => {
            if a.on("clear") {
                target_then(0, &mut params)?;
                params.insert("percent".into(), Value::Null);
            } else {
                let rest = target_then(1, &mut params)?;
                let pct: u64 = rest[0]
                    .parse()
                    .ok()
                    .filter(|p| *p <= 100)
                    .ok_or_else(|| usage_err(cmd, "percent must be 0 to 100"))?;
                params.insert("percent".into(), json!(pct));
            }
            if let Some(l) = a.val("label") {
                params.insert("label".into(), json!(l));
            }
        }
        "notify" => {
            if n != 1 {
                return Err(usage_err(cmd, "needs a body"));
            }
            if let Some(t) = a.val("title") {
                params.insert("title".into(), json!(t));
            }
            params.insert("body".into(), json!(pos[0]));
        }
        _ => unreachable!("every command in COMMANDS is handled"),
    }

    if let Some(c) = caller {
        params.insert("caller".into(), json!(c));
    }
    Ok(Cli::Call {
        command: cmd,
        request: Request {
            id: None,
            method: method.to_string(),
            params: Value::Object(params),
        },
        json: a.on("json"),
        stream: matches!(cmd, "events" | "wait"),
    })
}

/// A session id as `s4`, from a number or an already-formatted string.
fn id_text(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(format!("s{n}")),
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("id").and_then(id_text),
        _ => None,
    }
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// `/Users/me/w/api` → `~/w/api`, given the home folder.
fn tildify(path: &str, home: &str) -> String {
    if home.is_empty() {
        return path.to_string();
    }
    match path.strip_prefix(home) {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_string(),
    }
}

fn session_line(s: &Value) -> String {
    let mut line = id_text(&s["id"]).unwrap_or_else(|| "?".into());
    if let Some(n) = s.get("number").and_then(Value::as_u64) {
        line.push_str(&format!("  #{n}"));
    }
    for key in ["status", "name"] {
        if let Some(v) = str_of(s, key) {
            line.push_str(&format!("  {v}"));
        }
    }
    if let Some(f) = str_of(s, "project") {
        let home = std::env::var("HOME").unwrap_or_default();
        line.push_str(&format!("  [{}]", tildify(f, &home)));
    }
    if let Some(p) = str_of(s, "preset") {
        line.push_str(&format!("  ({p})"));
    }
    line
}

/// `HH:MM:SS` from unix milliseconds, shifted by a UTC offset in seconds.
fn clock_hms(ms: u64, utc_offset_secs: i64) -> String {
    let secs = (ms as i64 / 1000 + utc_offset_secs).rem_euclid(86_400);
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// macOS `struct tm` (<time.h>).
#[repr(C)]
struct Tm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: std::ffi::c_long,
    tm_zone: *const std::ffi::c_char,
}

unsafe extern "C" {
    fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
}

/// The machine's UTC offset at that moment (seconds east), 0 if it can't be read.
fn local_utc_offset(ms: u64) -> i64 {
    let t = (ms / 1000) as i64;
    let mut tm = std::mem::MaybeUninit::<Tm>::zeroed();
    // SAFETY: `t` and `tm` are valid for the call; localtime_r fills `tm` and returns it
    // (or null on failure, in which case we don't read it).
    unsafe {
        if localtime_r(&t, tm.as_mut_ptr()).is_null() {
            return 0;
        }
        tm.assume_init().tm_gmtoff as i64
    }
}

/// `HH:MM:SS` in local time.
fn clock(ms: u64) -> String {
    clock_hms(ms, local_utc_offset(ms))
}

fn event_line(e: &Value) -> String {
    let mut line = clock(e["ts"].as_u64().unwrap_or(0));
    if let Some(s) = e.get("session").filter(|s| s.is_object()) {
        line.push_str("  ");
        line.push_str(&id_text(&s["id"]).unwrap_or_else(|| "?".into()));
        if let Some(n) = s.get("number").and_then(Value::as_u64) {
            line.push_str(&format!(" #{n}"));
        }
        if let Some(name) = str_of(s, "name") {
            line.push_str(&format!(" {name}"));
        }
    }
    match (str_of(e, "from"), str_of(e, "to")) {
        (Some(from), Some(to)) => line.push_str(&format!("  {from} → {to}")),
        _ => {
            if let Some(kind) = str_of(e, "event") {
                line.push_str(&format!("  {kind}"));
            }
            if let Some(extra) = str_of(e, "title").or_else(|| str_of(e, "body")) {
                line.push_str(&format!(" {extra}"));
            }
        }
    }
    line
}

/// What to print for a successful reply (no trailing newline; empty means print nothing).
pub fn render(command: &str, result: &Value, json: bool) -> String {
    if json {
        return result.to_string();
    }
    match command {
        "ls" => {
            let list = result.get("sessions").unwrap_or(result);
            list.as_array()
                .map(|a| a.iter().map(session_line).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default()
        }
        "new" | "wait" => id_text(result).unwrap_or_default(),
        "read" | "question" | "status" => match result {
            Value::String(s) => s.clone(),
            Value::Object(_) => str_of(result, "text")
                .or_else(|| str_of(result, "status"))
                .unwrap_or_default()
                .to_string(),
            _ => String::new(),
        },
        "events" => event_line(result),
        "ping" => "pong".to_string(),
        "presets" => result
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().or_else(|| str_of(p, "name")))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        "version" => match result {
            Value::String(s) => s.clone(),
            _ => {
                let v = str_of(result, "version").unwrap_or("?");
                let methods: Vec<&str> = result["methods"]
                    .as_array()
                    .map(|m| m.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                if methods.is_empty() {
                    v.to_string()
                } else {
                    format!("{v}\n{}", methods.join("\n"))
                }
            }
        },
        _ => String::new(),
    }
}

/// The process exit code for an error reply's `code`.
pub fn exit_code(error_code: &str) -> i32 {
    match error_code {
        "timeout" => 124,
        "bad_request" | "unknown_method" => 2,
        "unavailable" => 3,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Cli, String> {
        parse(
            &a.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            Some(3),
        )
    }
    fn call(c: Result<Cli, String>) -> (Request, bool, bool) {
        match c.unwrap() {
            Cli::Call {
                request,
                json,
                stream,
                ..
            } => (request, json, stream),
            _ => panic!("not a call"),
        }
    }

    #[test]
    fn only_no_args_or_psn_opens_the_app() {
        assert!(matches!(p(&[]), Ok(Cli::Gui)));
        assert!(matches!(p(&["-psn_0_123"]), Ok(Cli::Gui)));
        assert!(matches!(p(&["mcp"]), Ok(Cli::Mcp)));
    }

    #[test]
    fn an_unknown_command_is_an_error_not_a_window() {
        let e = p(&["lss"]).unwrap_err();
        assert!(e.contains("unknown command: lss"), "{e}");
        assert!(e.contains("fuxx help"), "{e}");
        assert!(p(&["--frobnicate"]).is_err());
        assert!(p(&["--json"]).is_err());
    }

    #[test]
    fn json_before_the_command_works_like_after() {
        let (r, json, _) = call(p(&["--json", "ls"]));
        assert_eq!(r.method, "session.list");
        assert!(json);
        match p(&["--json", "send", "s4", "--", "--enter"]).unwrap() {
            Cli::Call {
                command,
                request,
                json,
                ..
            } => {
                assert_eq!(command, "send");
                assert!(json);
                assert_eq!(request.params["text"], "--enter");
            }
            _ => panic!("not a call"),
        }
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
        assert_eq!(
            (r.method.as_str(), &r.params["text"], &r.params["enter"]),
            ("session.send", &"hi".into(), &false.into())
        );
        assert!(r.params["target"].is_null());
        let (r, _, _) = call(p(&["send", "s4", "hi there", "--enter"]));
        assert_eq!(
            (&r.params["target"], &r.params["enter"]),
            (&"s4".into(), &true.into())
        );
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
        let (r, _, _) = call(p(&[
            "new",
            "--preset",
            "claude",
            "--split",
            "right",
            "--background",
        ]));
        assert_eq!(r.method, "session.new");
        assert_eq!(
            (
                &r.params["preset"],
                &r.params["split"],
                &r.params["background"]
            ),
            (&"claude".into(), &"right".into(), &true.into())
        );
        let (r, _, stream) = call(p(&["wait", "--any", "--for", "done", "--timeout", "30"]));
        assert!(stream);
        assert_eq!(
            (&r.params["any"], &r.params["for"], &r.params["timeout_ms"]),
            (&true.into(), &"done".into(), &30000.into())
        );
        assert!(p(&["new", "--split", "sideways"]).is_err());
        assert!(p(&["send"]).is_err());
    }

    #[test]
    fn exit_codes() {
        assert_eq!(
            (
                exit_code("refused"),
                exit_code("timeout"),
                exit_code("bad_request"),
                exit_code("unavailable")
            ),
            (1, 124, 2, 3)
        );
    }

    #[test]
    fn more_commands_and_bad_usage() {
        let (r, _, _) = call(p(&["wait", "--timeout", "1.5"]));
        assert_eq!(r.params["timeout_ms"], 1500);
        let (r, _, _) = call(p(&["progress", "s2", "40", "--label", "build"]));
        assert_eq!(
            (&r.params["target"], &r.params["percent"]),
            (&"s2".into(), &40.into())
        );
        let (r, _, _) = call(p(&["note", "--clear"]));
        assert!(r.params["text"].is_null());
        assert!(p(&["progress", "101"]).is_err());
        assert!(p(&["ls", "extra"]).is_err());
        assert!(p(&["send", "hi", "--bogus"]).is_err());
        assert!(matches!(p(&["help"]), Ok(Cli::Help(u)) if u.contains("equalize")));
    }

    #[test]
    fn render_text_output() {
        let ls = json!([{"id": 4, "number": 2, "status": "waiting", "name": "api", "project": "/x/api", "preset": "claude"}]);
        assert_eq!(
            render("ls", &ls, false),
            "s4  #2  waiting  api  [/x/api]  (claude)"
        );
        assert_eq!(render("new", &json!({"id": 5}), false), "s5");
        assert_eq!(render("status", &json!("done"), false), "done");
        assert_eq!(render("send", &json!({}), false), "");
        assert_eq!(render("ping", &json!({"pong": true}), false), "pong");
        assert_eq!(
            render("ping", &json!({"pong": true}), true),
            r#"{"pong":true}"#
        );
        assert_eq!(render("ls", &ls, true), ls.to_string());
        let ev = json!({"event": "status", "session": {"id": 4, "number": 2, "name": "api"}, "from": "working", "to": "waiting", "ts": 43_263_000u64});
        let line = render("events", &ev, false);
        assert!(line.ends_with("  s4 #2 api  working → waiting"), "{line}");
    }

    #[test]
    fn home_is_shortened_to_tilde() {
        assert_eq!(tildify("/Users/me/w/api", "/Users/me"), "~/w/api");
        assert_eq!(tildify("/Users/me", "/Users/me"), "~");
        assert_eq!(tildify("/Users/mex/w", "/Users/me"), "/Users/mex/w");
        assert_eq!(tildify("/x/api", ""), "/x/api");
    }

    #[test]
    fn clock_uses_the_utc_offset_and_wraps() {
        let ms = 43_263_000u64; // 12:01:03 UTC
        assert_eq!(clock_hms(ms, 0), "12:01:03");
        assert_eq!(clock_hms(ms, 7 * 3600), "19:01:03");
        assert_eq!(clock_hms(ms + 8 * 3600 * 1000, 7 * 3600), "03:01:03");
        assert_eq!(clock_hms(ms, -13 * 3600), "23:01:03");
    }

    #[test]
    fn double_dash_ends_options() {
        let (r, _, _) = call(p(&["send", "--", "--resume"]));
        assert_eq!(r.params["text"], "--resume");
        let (r, _, _) = call(p(&["answer", "s4", "--", "-1"]));
        assert_eq!(
            (&r.params["target"], &r.params["text"]),
            (&"s4".into(), &"-1".into())
        );
        assert!(p(&["send", "--resume"]).is_err());
    }

    #[test]
    fn command_table() {
        use serde_json::json;
        // (args, method, expected params; keys listed as null must be absent or null)
        let cases: Vec<(Vec<&str>, &str, Value)> = vec![
            (
                vec![
                    "new", "--folder", "D", "--run", "CMD", "--name", "N", "--split", "down",
                    "--of", "s2",
                ],
                "session.new",
                json!({"folder": "D", "preset": null, "run": "CMD", "name": "N", "split": "down", "of": "s2", "background": false, "caller": 3}),
            ),
            (
                vec!["read", "--lines", "5"],
                "session.read",
                json!({"lines": 5, "all": false, "caller": 3}),
            ),
            (
                vec!["read", "--all"],
                "session.read",
                json!({"lines": null, "all": true, "caller": 3}),
            ),
            (
                vec!["read", "--lines=7"],
                "session.read",
                json!({"lines": 7, "all": false, "caller": 3}),
            ),
            (
                vec!["answer", "s4", "y", "--force"],
                "session.answer",
                json!({"target": "s4", "text": "y", "force": true, "caller": 3}),
            ),
            (
                vec!["rename", ""],
                "session.rename",
                json!({"target": null, "name": "", "caller": 3}),
            ),
            (
                vec!["rename", "s4", "api"],
                "session.rename",
                json!({"target": "s4", "name": "api", "caller": 3}),
            ),
            (
                vec!["notify", "--title", "T", "body"],
                "notify",
                json!({"title": "T", "body": "body", "caller": 3}),
            ),
            (
                vec!["paste"],
                "session.paste",
                json!({"text": null, "caller": 3}),
            ),
            (
                vec!["close"],
                "session.close",
                json!({"target": null, "caller": 3}),
            ),
            (
                vec!["close", "s1"],
                "session.close",
                json!({"target": "s1", "caller": 3}),
            ),
            (
                vec!["focus"],
                "session.focus",
                json!({"target": null, "caller": 3}),
            ),
            (
                vec!["focus", "#2"],
                "session.focus",
                json!({"target": "#2", "caller": 3}),
            ),
            (
                vec!["equalize"],
                "pane.equalize",
                json!({"target": null, "caller": 3}),
            ),
            (
                vec!["equalize", "api"],
                "pane.equalize",
                json!({"target": "api", "caller": 3}),
            ),
            (
                vec!["wait", "s4", "--text", "PASS"],
                "session.wait",
                json!({"target": "s4", "any": false, "text": "PASS", "caller": 3}),
            ),
            (
                vec!["key", "s4", "up"],
                "session.key",
                json!({"target": "s4", "keys": ["up"], "caller": 3}),
            ),
            (
                vec!["key", "enter"],
                "session.key",
                json!({"target": null, "keys": ["enter"], "caller": 3}),
            ),
            (
                vec!["note", "hi"],
                "session.note",
                json!({"target": null, "text": "hi", "caller": 3}),
            ),
            (
                vec!["note", "s4", "hi"],
                "session.note",
                json!({"target": "s4", "text": "hi", "caller": 3}),
            ),
            (
                vec!["note", "--clear"],
                "session.note",
                json!({"text": null, "caller": 3}),
            ),
            (
                vec!["progress", "40"],
                "session.progress",
                json!({"target": null, "percent": 40, "caller": 3}),
            ),
            (
                vec!["progress", "s4", "40", "--label", "x"],
                "session.progress",
                json!({"target": "s4", "percent": 40, "label": "x", "caller": 3}),
            ),
            (
                vec!["progress", "--clear"],
                "session.progress",
                json!({"percent": null, "caller": 3}),
            ),
        ];
        for (args, method, want) in cases {
            let (r, _, _) = call(p(&args));
            assert_eq!(r.method, method, "{args:?}");
            let got = r.params.as_object().unwrap();
            for (k, v) in want.as_object().unwrap() {
                assert_eq!(&r.params[k.as_str()], v, "{args:?} key {k}");
            }
            // No stray keys beyond the expected ones (null-valued ones may be absent).
            for k in got.keys() {
                assert!(want.get(k).is_some(), "{args:?} has unexpected key {k}");
            }
        }
        let (_, json, _) = call(p(&["send", "x", "--json", "--enter"]));
        assert!(json);
    }
}
