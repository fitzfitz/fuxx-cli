//! Scripting requests, answered by the workspace on the UI thread. Each method goes through the
//! same functions as the keyboard and mouse (open, split, close, select, rename, equalize), so
//! saving, the sidebar and the layout rules apply unchanged.
//!
//! Also the live side: events sent to every subscriber, and pending waits, checked on each
//! status change, each close, and every 200 ms while any is pending.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{App, Context, EntityId, Task, Window, prelude::*};
use serde_json::{Value, json};

use crate::api::{self, ApiError, Request, SessionRef};
use crate::api_events::{Event, EventSession, Wait, Watch, event_json, wait_reply};
use crate::api_server::{Conn, Incoming};
use crate::appearance::Appearance;
use crate::grid::TextScope;
use crate::notifier;
use crate::presets::Preset;
use crate::restore::{Launch, SavedSession};
use crate::split_tree::Axis;
use crate::status::Status;
use crate::workspace::Workspace;

/// How many non-blank lines `session.question` returns.
const QUESTION_LINES: usize = 12;
/// How long closing the last session (which quits fuxx) waits for its reply to be written.
const REPLY_FLUSH: Duration = Duration::from_millis(500);
/// How often pending waits are checked (text waits and deadlines have no event of their own).
const WAIT_TICK: Duration = Duration::from_millis(200);
/// How many lines a text wait searches.
const WAIT_TEXT_LINES: usize = 200;

/// What the request loop sends for a request.
enum Reply {
    /// This reply line, now.
    Now(Result<Value, ApiError>),
    /// Nothing yet: a pending wait sends its own reply when it ends.
    Later,
}

impl Workspace {
    /// Answer the socket's requests one by one, on the UI thread, for as long as the workspace
    /// lives (dropping the task stops it).
    pub(crate) fn serve_requests(
        mut requests: UnboundedReceiver<Incoming>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            while let Some(Incoming { request, conn }) = requests.next().await {
                let mut close_later = None;
                let reply = this
                    .update_in(cx, |this, window, cx| {
                        this.reply_to(&request, &conn, &mut close_later, window, cx)
                    })
                    .unwrap_or_else(|_| Reply::Now(Err(ApiError::unavailable("fuxx is closing"))));
                if let Reply::Now(result) = reply {
                    conn.send_line(&api::reply(request.id, result));
                }
                // Closing the last session quits fuxx, so it waits (briefly) for the reply to
                // reach the client first.
                if let Some(view) = close_later {
                    let flushing = conn.clone();
                    cx.background_spawn(async move { flushing.flushed(REPLY_FLUSH) })
                        .await;
                    this.update_in(cx, |this, window, cx| this.close_view(view, window, cx))
                        .ok();
                }
            }
        })
    }

    /// What to send for one request: waits may reply later; everything else replies now.
    fn reply_to(
        &mut self,
        request: &Request,
        conn: &Conn,
        close_later: &mut Option<EntityId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        match request.method.as_str() {
            "session.wait" => match self.start_wait(request, conn, cx) {
                Ok(Some(result)) => Reply::Now(Ok(result)),
                Ok(None) => Reply::Later,
                Err(e) => Reply::Now(Err(e)),
            },
            "events.subscribe" => {
                // Added now and acknowledged right after, on this same UI-thread turn, so no
                // event can come before the acknowledgement.
                self.subscribers.retain(|c| !c.peer_gone());
                self.subscribers.push(conn.clone());
                Reply::Now(Ok(json!({ "subscribed": true })))
            }
            _ => Reply::Now(self.answer(request, close_later, window, cx)),
        }
    }

    /// The reply to one request. `close_later` is set instead of closing the last session,
    /// which the caller does once the reply is written.
    fn answer(
        &mut self,
        request: &Request,
        close_later: &mut Option<EntityId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, ApiError> {
        let p = &request.params;
        match request.method.as_str() {
            "system.ping" => Ok(json!({ "pong": true })),
            "system.version" => Ok(json!({
                "version": env!("CARGO_PKG_VERSION"),
                "methods": api::METHODS,
            })),
            "session.list" => Ok(self.list()),
            "session.new" => self.api_new(p, window, cx),
            "session.close" => {
                let i = self.target_index(p, "target")?;
                if self.sessions.items().len() == 1 {
                    *close_later = Some(self.sessions.items()[i].view.entity_id());
                } else {
                    self.close_at(i, window, cx);
                }
                Ok(json!({}))
            }
            "session.focus" => {
                let i = self.target_index(p, "target")?;
                // Even when it's already active: the point is to show it, so fuxx comes forward.
                self.sessions.select(i);
                self.activate(window, cx);
                window.activate_window();
                cx.activate(true);
                Ok(json!({}))
            }
            "session.rename" => {
                let name = required_str(p, "name")?;
                let i = self.target_index(p, "target")?;
                self.rename_at(i, name, window, cx);
                Ok(json!({}))
            }
            "session.send" => {
                let text = required_str(p, "text")?;
                let i = self.target_index(p, "target")?;
                let mut bytes = text.as_bytes().to_vec();
                if bool_param(p, "enter")? {
                    bytes.push(b'\r');
                }
                self.type_into(i, &bytes, cx);
                Ok(json!({}))
            }
            "session.key" => {
                let i = self.target_index(p, "target")?;
                let app_cursor = self.sessions.items()[i].view.read(cx).app_cursor();
                let bytes = key_sequence(p.get("keys"), app_cursor)?;
                self.type_into(i, &bytes, cx);
                Ok(json!({}))
            }
            "session.paste" => {
                let text = required_str(p, "text")?;
                let i = self.target_index(p, "target")?;
                let view = self.sessions.items()[i].view.clone();
                view.update(cx, |view, cx| view.paste(text, cx));
                Ok(json!({}))
            }
            "session.read" => {
                let scope = text_scope(p)?;
                let i = self.target_index(p, "target")?;
                let text = self.sessions.items()[i].view.read(cx).text(scope);
                Ok(json!({ "text": text }))
            }
            "session.status" => {
                let i = self.target_index(p, "target")?;
                Ok(json!({ "status": self.sessions.items()[i].status.as_str() }))
            }
            "session.question" => {
                let i = self.target_index(p, "target")?;
                let session = &self.sessions.items()[i];
                if session.status != Status::Waiting {
                    return Err(not_waiting(session.id, session.status));
                }
                let text = session.view.read(cx).text(TextScope::Last(QUESTION_LINES));
                Ok(json!({ "text": text }))
            }
            "session.answer" => {
                let text = required_str(p, "text")?;
                let force = bool_param(p, "force")?;
                let i = self.target_index(p, "target")?;
                let session = &self.sessions.items()[i];
                if session.status != Status::Waiting && !force {
                    return Err(not_waiting(session.id, session.status));
                }
                let bytes = format!("{text}\r").into_bytes();
                self.type_into(i, &bytes, cx);
                Ok(json!({}))
            }
            "pane.equalize" => {
                let i = self.target_index(p, "target")?;
                let pane = self.sessions.items()[i].view.entity_id();
                self.equalize_layout(pane, cx);
                Ok(json!({}))
            }
            "preset.list" => {
                let presets = &cx.global::<Appearance>().presets;
                Ok(json!(
                    presets
                        .iter()
                        .map(|p| json!({ "name": p.name, "command": p.command }))
                        .collect::<Vec<_>>()
                ))
            }
            "notify" => {
                let body = required_str(p, "body")?.to_string();
                let title = opt_str(p, "title")?.unwrap_or("fuxx").to_string();
                self.emit(
                    &Event::Notification {
                        title: &title,
                        body: &body,
                    },
                    None,
                );
                cx.background_spawn(async move { notifier::fire(&title, &body) })
                    .detach();
                Ok(json!({}))
            }
            // session.wait and events.subscribe are answered in `reply_to`.
            "session.note" | "session.progress" => Err(ApiError::unavailable(format!(
                "{} is coming in a later update",
                request.method
            ))),
            other => Err(ApiError::unknown_method(format!(
                "unknown method \"{other}\" (see system.version)"
            ))),
        }
    }

    /// Every session in sidebar order, as `session.list` reports it.
    fn list(&self) -> Value {
        let active = self.sessions.active_index();
        let shown = self.shown_layout();
        let entries: Vec<Value> = self
            .sessions
            .items()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let label = s.label();
                list_entry(&ListEntry {
                    id: s.id,
                    number: i + 1,
                    name: &label,
                    title: s.title.as_deref(),
                    status: s.status,
                    project: s.project.as_deref(),
                    preset: s.preset.as_ref().map(|p| p.name.as_str()),
                    focused: active == Some(i),
                    on_screen: shown.is_some_and(|t| t.contains(s.view.entity_id())),
                })
            })
            .collect();
        Value::Array(entries)
    }

    /// The sidebar index of the session `p[key]` names; no target means the caller, else the
    /// focused session.
    fn target_index(&self, p: &Value, key: &str) -> Result<usize, ApiError> {
        let target = opt_str(p, key)?.map(api::parse_target);
        let refs: Vec<SessionRef> = self
            .sessions
            .items()
            .iter()
            .map(|s| SessionRef {
                id: s.id,
                label: s.label().to_string(),
            })
            .collect();
        let focused = self.sessions.active().map(|s| s.id);
        let id = api::resolve(target.as_ref(), &refs, caller_param(p)?, focused)?;
        self.sessions
            .position(|s| s.id == id)
            .ok_or_else(|| ApiError::not_found(format!("no session s{id}")))
    }

    fn close_view(&mut self, view: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.sessions.position(|s| s.view.entity_id() == view) {
            self.close_at(i, window, cx);
        }
    }

    fn type_into(&self, index: usize, bytes: &[u8], cx: &mut Context<Self>) {
        let view = self.sessions.items()[index].view.clone();
        view.update(cx, |view, cx| view.type_bytes(bytes, cx));
    }

    /// Like the rename box's Enter, but exact: empty goes back to the automatic name. A rename
    /// box open on the same session is closed.
    fn rename_at(&mut self, index: usize, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        let old = session.name.clone();
        session.name = (!name.is_empty()).then(|| name.to_string());
        let renamed = session.name != old;
        let view = session.view.entity_id();
        if self.renaming.as_ref().is_some_and(|(v, _)| *v == view) {
            self.renaming = None;
            self.focus_active(window, cx);
        }
        if renamed {
            self.emit_renamed(index);
        }
        self.save();
        cx.notify();
    }

    /// `session.wait`: the reply if the wait is already over (met, timed out, or bad), else
    /// `None` with the wait stored to reply later.
    fn start_wait(
        &mut self,
        request: &Request,
        conn: &Conn,
        cx: &mut Context<Self>,
    ) -> Result<Option<Value>, ApiError> {
        let p = &request.params;
        let watch = if bool_param(p, "any")? {
            if opt_str(p, "target")?.is_some() {
                return Err(ApiError::bad_request("any takes no target"));
            }
            Watch::Any
        } else {
            let i = self.target_index(p, "target")?;
            Watch::One(self.sessions.items()[i].id)
        };
        let cond = api::parse_wait(opt_str(p, "for")?, opt_str(p, "text")?)?;
        let now = Instant::now();
        // Too far ahead to represent counts as no limit.
        let deadline = timeout_param(p)?.and_then(|t| now.checked_add(t));
        let sessions = self.statuses();
        let mut wait = Wait::new(conn.clone(), request.id, watch, cond, &sessions, deadline);
        let items = self.sessions.items();
        let mut screen = |id: u64| screen_text(items, id, cx);
        if let Some(outcome) = wait.outcome(now, &sessions, &[], &mut screen) {
            return wait_reply(outcome).map(Some);
        }
        self.waits.push(wait);
        self.tick_waits(cx);
        Ok(None)
    }

    /// Check every pending wait; `closed`: sessions that just closed. Finished waits are
    /// answered; their connections are returned (so a quit can let the replies out first).
    pub(crate) fn check_waits(&mut self, closed: &[u64], cx: &App) -> Vec<Conn> {
        if self.waits.is_empty() {
            return Vec::new();
        }
        let sessions = self.statuses();
        let items = self.sessions.items();
        let screen = |id: u64| screen_text(items, id, cx);
        let finished = self
            .waits
            .check(Instant::now(), &sessions, closed, screen, Conn::peer_gone);
        finished
            .into_iter()
            .map(|(wait, outcome)| {
                wait.conn
                    .send_line(&api::reply(wait.request, wait_reply(outcome)));
                wait.conn
            })
            .collect()
    }

    /// Start the 200 ms check unless it's running; it stops once no wait is pending.
    fn tick_waits(&mut self, cx: &mut Context<Self>) {
        if self.waits_ticking || self.waits.is_empty() {
            return;
        }
        self.waits_ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(WAIT_TICK).await;
                let pending = this
                    .update(cx, |this, cx| {
                        this.check_waits(&[], cx);
                        this.waits_ticking = !this.waits.is_empty();
                        this.waits_ticking
                    })
                    .unwrap_or(false);
                if !pending {
                    break;
                }
            }
        })
        .detach();
    }

    /// Every session's (id, status), in sidebar order.
    fn statuses(&self) -> Vec<(u64, Status)> {
        self.sessions
            .items()
            .iter()
            .map(|s| (s.id, s.status))
            .collect()
    }

    /// Send an event about session `index` (or none) to every subscriber; those that are gone
    /// or can't take it are dropped. Nothing while restoring.
    pub(crate) fn emit(&mut self, event: &Event, index: Option<usize>) {
        if self.subscribers.is_empty() || self.restoring {
            return;
        }
        let session = index.and_then(|i| self.sessions.items().get(i).map(|s| (i, s)));
        let label = session.map(|(_, s)| s.label());
        let about = session
            .zip(label.as_deref())
            .map(|((i, s), name)| EventSession {
                id: s.id,
                number: i + 1,
                name,
            });
        let line = event_json(event, about.as_ref(), now_ms()).to_string();
        self.subscribers
            .retain(|c| !c.peer_gone() && c.send_line(&line));
    }

    /// `session.renamed` for session `index`, with its new label.
    pub(crate) fn emit_renamed(&mut self, index: usize) {
        if let Some(name) = self.sessions.items().get(index).map(|s| s.label()) {
            self.emit(&Event::Renamed { name: &name }, Some(index));
        }
    }

    /// `focus.changed` when the active session is another one than last time.
    pub(crate) fn note_focus(&mut self) {
        let focused = self.sessions.active().map(|s| s.id);
        if focused != self.focused {
            self.focused = focused;
            if focused.is_some() {
                self.emit(&Event::FocusChanged, self.sessions.active_index());
            }
        }
    }

    /// Close the window (quitting fuxx) once `conns` have written what's queued for them, for
    /// at most `REPLY_FLUSH`; at once when there's nothing to wait for.
    pub(crate) fn quit_after_flush(
        &mut self,
        conns: Vec<Conn>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let conns: Vec<Conn> = conns.into_iter().filter(|c| !c.peer_gone()).collect();
        if conns.is_empty() {
            window.remove_window();
            return;
        }
        cx.spawn_in(window, async move |_, cx| {
            cx.background_spawn(async move {
                let deadline = Instant::now() + REPLY_FLUSH;
                for conn in conns {
                    conn.flushed(deadline.saturating_duration_since(Instant::now()));
                }
            })
            .await;
            cx.update(|window, _| window.remove_window()).ok();
        })
        .detach();
    }

    /// `session.new`: a session like ⌘T / a preset, or with `split` a pane like ⌘D beside `of`.
    fn api_new(
        &mut self,
        p: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, ApiError> {
        let preset = match opt_str(p, "preset")? {
            Some(name) => Some(find_preset(&cx.global::<Appearance>().presets, name)?),
            None => None,
        };
        let split = opt_str(p, "split")?.map(split_axis).transpose()?;
        let folder = opt_str(p, "folder")?.map(PathBuf::from);
        if let Some(dir) = &folder {
            check_folder(dir)?;
        }
        let run = opt_str(p, "run")?
            .filter(|r| !r.is_empty())
            .map(str::to_string);
        let background = bool_param(p, "background")?;
        let name = opt_str(p, "name")?
            .filter(|n| !n.is_empty())
            .map(str::to_string);
        let of = match split {
            Some(_) => Some(self.target_index(p, "of")?),
            None if opt_str(p, "of")?.is_some() => {
                return Err(ApiError::bad_request("of needs split"));
            }
            None => None,
        };
        let project = folder
            .or_else(|| of.and_then(|i| self.sessions.items()[i].project.clone()))
            .or_else(|| self.active_project());
        // By id: the new session's row can push the previous one down.
        let previous = self.sessions.active().map(|s| s.id);
        // The split path splits the active session, so `of` becomes it first.
        if let Some(i) = of {
            self.sessions.select(i);
        }
        let id = self.next_id;
        let launch = Launch {
            saved: SavedSession {
                name,
                project,
                preset,
            },
            run_preset: true,
        };
        self.open_session(launch, split, window, cx);
        let index_of = |this: &Self, id: u64| this.sessions.position(|s| s.id == id);
        let Some(index) = index_of(self, id) else {
            // Nothing opened (logged); undo the selection of `of`.
            if let Some(i) = previous.and_then(|p| index_of(self, p)) {
                self.sessions.select(i);
            }
            return Err(ApiError::refused("could not start a shell"));
        };
        if let Some(run) = run {
            self.type_into(index, format!("{run}\r").as_bytes(), cx);
        }
        if background && let Some(i) = previous.and_then(|p| index_of(self, p)) {
            self.select(i, window, cx);
        }
        Ok(json!({ "id": format!("s{id}") }))
    }
}

/// What `session.list` reports for one session.
struct ListEntry<'a> {
    id: u64,
    number: usize,
    name: &'a str,
    title: Option<&'a str>,
    status: Status,
    project: Option<&'a Path>,
    preset: Option<&'a str>,
    focused: bool,
    on_screen: bool,
}

fn list_entry(e: &ListEntry) -> Value {
    json!({
        "id": format!("s{}", e.id),
        "number": e.number,
        "name": e.name,
        "title": e.title,
        "status": e.status.as_str(),
        "project": e.project.map(|p| p.display().to_string()),
        "preset": e.preset,
        "focused": e.focused,
        "on_screen": e.on_screen,
        // Set by session.note / session.progress (a later update).
        "note": null,
        "progress": null,
    })
}

/// The calling session from `caller`: the number of `FUXX_SESSION`'s `s<N>`, or absent.
fn caller_param(p: &Value) -> Result<Option<u64>, ApiError> {
    match p.get("caller") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| ApiError::bad_request("caller must be a session number")),
    }
}

/// `p[key]` as text; absent or null is `None`, anything else is a bad request.
fn opt_str<'a>(p: &'a Value, key: &str) -> Result<Option<&'a str>, ApiError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(ApiError::bad_request(format!("{key} must be text"))),
    }
}

fn required_str<'a>(p: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    opt_str(p, key)?.ok_or_else(|| ApiError::bad_request(format!("{key} is missing")))
}

/// A switch: absent or null is off; anything but true/false is a bad request.
fn bool_param(p: &Value, key: &str) -> Result<bool, ApiError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(ApiError::bad_request(format!(
            "{key} must be true or false"
        ))),
    }
}

/// `timeout_ms`: how long a wait may take; absent or null is no limit.
fn timeout_param(p: &Value) -> Result<Option<Duration>, ApiError> {
    match p.get("timeout_ms") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(|ms| Some(Duration::from_millis(ms)))
            .ok_or_else(|| {
                ApiError::bad_request("timeout_ms must be a whole number of milliseconds")
            }),
    }
}

/// `lines` → the last n lines, else `all` → everything, else the screen.
fn text_scope(p: &Value) -> Result<TextScope, ApiError> {
    match p.get("lines") {
        None | Some(Value::Null) => {}
        Some(n) => {
            let n = n
                .as_u64()
                .ok_or_else(|| ApiError::bad_request("lines must be a whole number"))?;
            return Ok(TextScope::Last(n as usize));
        }
    }
    Ok(if bool_param(p, "all")? {
        TextScope::All
    } else {
        TextScope::Screen
    })
}

fn split_axis(s: &str) -> Result<Axis, ApiError> {
    match s {
        "right" => Ok(Axis::Row),
        "down" => Ok(Axis::Column),
        _ => Err(ApiError::bad_request("split must be right or down")),
    }
}

/// All the keys' bytes, or a bad request naming the first unknown key (so nothing is typed).
fn key_sequence(keys: Option<&Value>, app_cursor: bool) -> Result<Vec<u8>, ApiError> {
    let names = keys
        .and_then(Value::as_array)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| ApiError::bad_request("keys must be a list of key names"))?;
    let mut bytes = Vec::new();
    for name in names {
        let name = name
            .as_str()
            .ok_or_else(|| ApiError::bad_request("keys must be a list of key names"))?;
        let key = api::key_bytes(name, app_cursor)
            .ok_or_else(|| ApiError::bad_request(format!("unknown key \"{name}\"")))?;
        bytes.extend_from_slice(&key);
    }
    Ok(bytes)
}

/// A preset by name: exact, else ignoring case.
fn find_preset(presets: &[Preset], name: &str) -> Result<Preset, ApiError> {
    let lower = name.to_lowercase();
    presets
        .iter()
        .find(|p| p.name == name)
        .or_else(|| presets.iter().find(|p| p.name.to_lowercase() == lower))
        .cloned()
        .ok_or_else(|| ApiError::not_found(format!("no preset named \"{name}\"")))
}

/// A new session's folder must be an existing folder, given as a full path.
fn check_folder(dir: &Path) -> Result<(), ApiError> {
    if dir.is_relative() {
        return Err(ApiError::bad_request(format!(
            "folder must be a full path: {}",
            dir.display()
        )));
    }
    if !dir.is_dir() {
        return Err(ApiError::not_found(format!("no folder {}", dir.display())));
    }
    Ok(())
}

/// The last lines of session `id`'s screen (empty if it's gone), for text waits.
fn screen_text(sessions: &[crate::workspace::Session], id: u64, cx: &App) -> String {
    sessions
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.view.read(cx).text(TextScope::Last(WAIT_TEXT_LINES)))
        .unwrap_or_default()
}

/// Now, in unix milliseconds (the events' `ts`).
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn not_waiting(id: u64, status: Status) -> ApiError {
    ApiError::refused(format!("s{id} is not waiting (it is {})", status.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli;

    #[test]
    fn list_entry_shape_and_how_ls_prints_it() {
        let e = ListEntry {
            id: 4,
            number: 2,
            name: "api",
            title: Some("claude"),
            status: Status::Waiting,
            project: Some(Path::new("/x/api")),
            preset: Some("claude"),
            focused: true,
            on_screen: true,
        };
        let v = list_entry(&e);
        assert_eq!(
            v,
            json!({
                "id": "s4", "number": 2, "name": "api", "title": "claude", "status": "waiting",
                "project": "/x/api", "preset": "claude", "focused": true, "on_screen": true,
                "note": null, "progress": null,
            })
        );
        assert_eq!(
            cli::render("ls", &json!([v]), false),
            "s4  #2  waiting  api  [/x/api]  (claude)"
        );
        let bare = list_entry(&ListEntry {
            title: None,
            project: None,
            preset: None,
            focused: false,
            on_screen: false,
            ..e
        });
        assert_eq!(
            (&bare["title"], &bare["project"], &bare["preset"]),
            (&Value::Null, &Value::Null, &Value::Null)
        );
    }

    #[test]
    fn caller_is_a_number_or_absent() {
        assert_eq!(caller_param(&json!({"caller": 3})), Ok(Some(3)));
        assert_eq!(caller_param(&json!({"caller": null})), Ok(None));
        assert_eq!(caller_param(&json!({})), Ok(None));
        assert_eq!(caller_param(&Value::Null), Ok(None));
        for bad in [json!("s7"), json!("7"), json!(-1), json!(1.5), json!(true)] {
            assert_eq!(
                caller_param(&json!({ "caller": bad })).unwrap_err().code,
                "bad_request",
                "{bad}"
            );
        }
    }

    #[test]
    fn params_text_and_switches() {
        let p = json!({"text": "hi", "n": 3, "enter": true, "no": false, "off": null});
        assert_eq!(opt_str(&p, "text"), Ok(Some("hi")));
        assert_eq!(opt_str(&p, "off"), Ok(None));
        assert_eq!(opt_str(&p, "missing"), Ok(None));
        assert_eq!(opt_str(&p, "n").unwrap_err().code, "bad_request");
        assert_eq!(required_str(&p, "text"), Ok("hi"));
        assert_eq!(required_str(&p, "off").unwrap_err().code, "bad_request");
        assert_eq!(bool_param(&p, "enter"), Ok(true));
        assert_eq!(bool_param(&p, "no"), Ok(false));
        assert_eq!(bool_param(&p, "missing"), Ok(false));
        assert_eq!(bool_param(&p, "off"), Ok(false));
        assert_eq!(bool_param(&p, "text").unwrap_err().code, "bad_request");
        assert_eq!(bool_param(&p, "n").unwrap_err().code, "bad_request");
    }

    #[test]
    fn wait_timeouts() {
        assert_eq!(timeout_param(&json!({})), Ok(None));
        assert_eq!(timeout_param(&json!({"timeout_ms": null})), Ok(None));
        assert_eq!(
            timeout_param(&json!({"timeout_ms": 1500})),
            Ok(Some(Duration::from_millis(1500)))
        );
        assert_eq!(
            timeout_param(&json!({"timeout_ms": 0})),
            Ok(Some(Duration::ZERO))
        );
        for bad in [json!(-1), json!(1.5), json!("30"), json!(true)] {
            assert_eq!(
                timeout_param(&json!({ "timeout_ms": bad }))
                    .unwrap_err()
                    .code,
                "bad_request",
                "{bad}"
            );
        }
    }

    #[test]
    fn read_scopes() {
        assert_eq!(text_scope(&json!({})), Ok(TextScope::Screen));
        assert_eq!(
            text_scope(&json!({"lines": null, "all": false})),
            Ok(TextScope::Screen)
        );
        assert_eq!(text_scope(&json!({"all": true})), Ok(TextScope::All));
        assert_eq!(text_scope(&json!({"lines": 5})), Ok(TextScope::Last(5)));
        assert_eq!(
            text_scope(&json!({"lines": 5, "all": true})),
            Ok(TextScope::Last(5))
        );
        assert_eq!(
            text_scope(&json!({"all": "yes"})).unwrap_err().code,
            "bad_request"
        );
        assert_eq!(
            text_scope(&json!({"lines": "5"})).unwrap_err().code,
            "bad_request"
        );
        assert_eq!(
            text_scope(&json!({"lines": -1})).unwrap_err().code,
            "bad_request"
        );
    }

    #[test]
    fn split_directions() {
        assert_eq!(split_axis("right"), Ok(Axis::Row));
        assert_eq!(split_axis("down"), Ok(Axis::Column));
        assert_eq!(split_axis("left").unwrap_err().code, "bad_request");
    }

    #[test]
    fn keys_all_or_nothing() {
        assert_eq!(
            key_sequence(Some(&json!(["ctrl-c", "enter"])), false),
            Ok(vec![3, b'\r'])
        );
        assert_eq!(
            key_sequence(Some(&json!(["up"])), true),
            Ok(b"\x1bOA".to_vec())
        );
        let e = key_sequence(Some(&json!(["enter", "nope"])), false).unwrap_err();
        assert_eq!(e.code, "bad_request");
        assert!(e.message.contains("nope"), "{}", e.message);
        for bad in [json!([]), json!("enter"), json!([1]), Value::Null] {
            assert_eq!(
                key_sequence(Some(&bad), false).unwrap_err().code,
                "bad_request",
                "{bad}"
            );
        }
        assert_eq!(key_sequence(None, false).unwrap_err().code, "bad_request");
    }

    #[test]
    fn presets_by_name() {
        let presets = vec![
            Preset {
                name: "claude".into(),
                command: "claude".into(),
            },
            Preset {
                name: "Claude".into(),
                command: "claude --resume".into(),
            },
        ];
        assert_eq!(
            find_preset(&presets, "Claude").unwrap().command,
            "claude --resume"
        );
        assert_eq!(find_preset(&presets, "CLAUDE").unwrap().command, "claude");
        assert_eq!(
            find_preset(&presets, "codex").unwrap_err().code,
            "not_found"
        );
    }

    #[test]
    fn folders_must_exist_and_be_absolute() {
        let tmp = std::env::temp_dir();
        assert_eq!(check_folder(&tmp), Ok(()));
        assert_eq!(
            check_folder(Path::new("relative/dir")).unwrap_err().code,
            "bad_request"
        );
        assert_eq!(
            check_folder(&tmp.join("fuxx-no-such-folder-9f3a"))
                .unwrap_err()
                .code,
            "not_found"
        );
    }
}
