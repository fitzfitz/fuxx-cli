//! Scripting requests, answered by the workspace on the UI thread. Each method goes through the
//! same functions as the keyboard and mouse (open, split, close, select, rename, equalize), so
//! saving, the sidebar and the layout rules apply unchanged.
//!
//! Also the live side: events sent to every subscriber, and pending waits, checked on each
//! status change, each close, and every 200 ms while any is pending.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{App, Context, EntityId, Task, Window, prelude::*};
use serde_json::{Value, json};

use crate::api::{self, ApiError, Request, SessionRef, WaitFor};
use crate::api_events::{Event, EventSession, Wait, Watch, event_json, wait_reply};
use crate::api_server::{Conn, Incoming};
use crate::appearance::Appearance;
use crate::daemon_client::DaemonClient;
use crate::grid::TextScope;
use crate::layout_model::TabId;
use crate::notification_list::Entry;
use crate::notifier;
use crate::presets::Preset;
use crate::restore::{Launch, SavedSession};
use crate::split_tree::Axis;
use crate::ssh_target::{SshTarget, validate_host};
use crate::status::Status;
use crate::tab_color::TabColor;
use crate::workspace::{At, Opened, Workspace};
use crate::workspace_daemon::{open_refusal, wait_ended};
use crate::workspace_ssh::Place;

/// How many non-blank lines `session.question` returns.
const QUESTION_LINES: usize = 12;
/// How long closing the last session (which quits fuxx) waits for its reply to be written.
const REPLY_FLUSH: Duration = Duration::from_millis(500);
/// How long closing the last session waits for the background service to have ended it.
const ENDING_WAIT: Duration = Duration::from_secs(2);
/// How often pending waits are checked (text waits and deadlines have no event of their own).
const WAIT_TICK: Duration = Duration::from_millis(200);
/// How many lines a text wait searches.
const WAIT_TEXT_LINES: usize = 200;

/// How long a request waits for a screen to arrive from the background service.
const SCREEN_WAIT: Duration = Duration::from_secs(10);
/// How often requests waiting for a screen look again.
const SCREEN_TICK: Duration = Duration::from_millis(20);

/// A request that reads a screen not arrived yet from the background service (a reopened
/// session not shown so far): answered once it has (`retry_deferred`), never from a blank one.
pub(crate) struct Deferred {
    request: Request,
    conn: Conn,
    since: Instant,
}

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

    /// What to send for one request: one reading a screen that hasn't arrived yet waits for
    /// it; otherwise `dispatch`.
    fn reply_to(
        &mut self,
        request: &Request,
        conn: &Conn,
        close_later: &mut Option<EntityId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        match self.screens_pending(request, cx) {
            Ok(ids) if ids.is_empty() => self.dispatch(request, conn, close_later, window, cx),
            Ok(_) => {
                self.deferred.push(Deferred {
                    request: request.clone(),
                    conn: conn.clone(),
                    since: Instant::now(),
                });
                self.tick_deferred(window, cx);
                Reply::Later
            }
            Err(e) => Reply::Now(Err(e)),
        }
    }

    /// The sessions whose screens `request` reads but which haven't arrived yet from the
    /// background service (each is asked for now); see `screens_to_wait_for`. A request that
    /// reads no screen, or names a bad target (its own answer says so), waits for nothing.
    fn screens_pending(&self, request: &Request, cx: &App) -> Result<Vec<u64>, ApiError> {
        let p = &request.params;
        let items = self.sessions.items();
        let target = || self.target_index(p, "target").ok();
        let (indices, any): (Vec<usize>, bool) = match request.method.as_str() {
            "session.read" | "session.question" => (target().into_iter().collect(), false),
            "session.wait" => {
                let cond = api::parse_wait(
                    opt_str(p, "for").ok().flatten(),
                    opt_str(p, "text").ok().flatten(),
                );
                if !matches!(cond, Ok(WaitFor::Text(_))) {
                    return Ok(Vec::new());
                }
                if bool_param(p, "any").unwrap_or(false) {
                    ((0..items.len()).collect(), true)
                } else {
                    (target().into_iter().collect(), false)
                }
            }
            _ => return Ok(Vec::new()),
        };
        let facts: Vec<(u64, bool, bool)> = indices
            .iter()
            .map(|&i| {
                (
                    items[i].id,
                    items[i].view.read(cx).is_drawn(),
                    items[i].lost,
                )
            })
            .collect();
        let ids = screens_to_wait_for(&facts, any)?;
        for s in items.iter().filter(|s| ids.contains(&s.id)) {
            s.view.read(cx).attach();
        }
        Ok(ids)
    }

    /// Look again at the deferred requests every `SCREEN_TICK` until none is left.
    fn tick_deferred(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.deferred_ticking || self.deferred.is_empty() {
            return;
        }
        self.deferred_ticking = true;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(SCREEN_TICK).await;
                let pending = this
                    .update_in(cx, |this, window, cx| {
                        this.retry_deferred(window, cx);
                        this.deferred_ticking = !this.deferred.is_empty();
                        this.deferred_ticking
                    })
                    .unwrap_or(false);
                if !pending {
                    break;
                }
            }
        })
        .detach();
    }

    /// Answer the deferred requests whose screens have arrived (or whose sessions closed or
    /// were lost meanwhile); one still waiting after `SCREEN_WAIT` gets an error.
    fn retry_deferred(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for d in std::mem::take(&mut self.deferred) {
            if d.conn.peer_gone() {
                continue;
            }
            let reply = match self.screens_pending(&d.request, cx) {
                Ok(ids) if ids.is_empty() => {
                    let mut close_later = None;
                    self.dispatch(&d.request, &d.conn, &mut close_later, window, cx)
                }
                Ok(_) if d.since.elapsed() < SCREEN_WAIT => {
                    self.deferred.push(d);
                    continue;
                }
                Ok(ids) => Reply::Now(Err(ApiError::unavailable(format!(
                    "s{}: its screen didn't arrive from the background service",
                    ids[0]
                )))),
                Err(e) => Reply::Now(Err(e)),
            };
            if let Reply::Now(result) = reply {
                d.conn.send_line(&api::reply(d.request.id, result));
            }
        }
    }

    /// What to send for one request: waits may reply later; everything else replies now.
    fn dispatch(
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
            // Replied once the session is open: with the background service, its id comes later.
            "session.new" => match self.api_new(request, conn, window, cx) {
                Ok(()) => Reply::Later,
                Err(e) => Reply::Now(Err(e)),
            },
            method if method.starts_with("browser.") => {
                match self.browser_request(request, conn, window, cx) {
                    Ok(Some(result)) => Reply::Now(Ok(result)),
                    Ok(None) => Reply::Later,
                    Err(e) => Reply::Now(Err(e)),
                }
            }
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
                "methods": ([api::METHODS, api::BROWSER_METHODS].concat()),
            })),
            "session.list" => Ok(self.list()),
            // session.new is answered in `reply_to`.
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
                // Even when it's already active: the point is to show it (its workspace, pane
                // and tab), so fuxx comes forward.
                self.select(i, window, cx);
                self.activate(window, cx);
                window.activate_window();
                cx.activate(true);
                Ok(json!({}))
            }
            "session.rename" => {
                let name = required_str(p, "name")?;
                let i = self.target_index(p, "target")?;
                self.rename_at(i, name, cx);
                Ok(json!({}))
            }
            "session.send" => {
                let text = required_str(p, "text")?;
                let i = self.input_target(p, "target")?;
                let mut bytes = text.as_bytes().to_vec();
                if bool_param(p, "enter")? {
                    bytes.push(b'\r');
                }
                self.type_into(i, &bytes, cx);
                Ok(json!({}))
            }
            "session.key" => {
                let i = self.input_target(p, "target")?;
                let app_cursor = self.sessions.items()[i].view.read(cx).app_cursor();
                let bytes = key_sequence(p.get("keys"), app_cursor)?;
                self.type_into(i, &bytes, cx);
                Ok(json!({}))
            }
            "session.paste" => {
                let text = required_str(p, "text")?;
                let i = self.input_target(p, "target")?;
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
                let i = self.live_target(p, "target")?;
                Ok(json!({ "status": self.sessions.items()[i].status.as_str() }))
            }
            "session.question" => {
                let i = self.live_target(p, "target")?;
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
                let i = self.input_target(p, "target")?;
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
                if let Some(ws) = self.workspace_of(i) {
                    self.equalize_layout(ws, cx);
                }
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
                let caller = caller_param(p)?;
                self.emit(
                    &Event::Notification {
                        title: &title,
                        body: &body,
                    },
                    None,
                );
                // Listed in the 🔔 too, about the terminal it ran in (if it ran in one).
                let entry = Entry::scripted(
                    &title,
                    &body,
                    caller,
                    |id| self.index_of(id).is_some(),
                    Instant::now(),
                );
                let terminal = entry.terminal;
                self.note_notification(entry);
                cx.notify();
                let about = self.banner_about(terminal);
                cx.background_spawn(async move { notifier::fire(&title, &body, about) })
                    .detach();
                Ok(json!({}))
            }
            // session.wait and events.subscribe are answered in `reply_to`.
            "session.note" => {
                let note = note_param(p)?;
                let i = self.target_index(p, "target")?;
                self.sessions
                    .get_mut(i)
                    .expect("target_index is in range")
                    .note = note;
                self.push_meta(i, cx);
                cx.notify();
                Ok(json!({}))
            }
            "session.progress" => {
                let progress = progress_param(p)?;
                let i = self.target_index(p, "target")?;
                self.sessions
                    .get_mut(i)
                    .expect("target_index is in range")
                    .progress = progress;
                self.push_meta(i, cx);
                cx.notify();
                Ok(json!({}))
            }
            other => Err(ApiError::unknown_method(format!(
                "unknown method \"{other}\" (see system.version)"
            ))),
        }
    }

    /// Every session in sidebar order, as `session.list` reports it.
    fn list(&self) -> Value {
        let active = self.sessions.active_index();
        let shown = self.on_screen();
        let entries: Vec<Value> = self
            .sessions
            .items()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let label = s.label();
                let (workspace, pane) = self.place_of(i);
                list_entry(&ListEntry {
                    id: s.id,
                    number: i + 1,
                    workspace,
                    pane,
                    name: &label,
                    title: s.title.as_deref(),
                    status: s.status,
                    project: s.project.as_deref(),
                    preset: s.preset.as_ref().map(|p| p.name.as_str()),
                    focused: active == Some(i),
                    on_screen: shown.contains(&s.id),
                    note: s.note.as_deref(),
                    progress: s.progress.as_ref(),
                    live: !s.lost,
                    ssh: s.ssh.as_ref().map(|t| t.host.as_str()),
                    color: self
                        .workspace_of(i)
                        .and_then(|ws| self.model.workspace(ws))
                        .and_then(|w| w.color),
                })
            })
            .collect();
        Value::Array(entries)
    }

    /// Session `index`'s workspace's place in the sidebar and its pane's place in that
    /// workspace's reading order, each from 1 (0: not shown, never by the model's rules).
    fn place_of(&self, index: usize) -> (usize, usize) {
        let Some(ws) = self.workspace_of(index) else {
            return (0, 0);
        };
        let pane = self.pane_of(index).and_then(|pane| {
            let leaves = self.model.workspace(ws)?.layout.leaves();
            leaves.iter().position(|&p| p == pane)
        });
        (
            self.workspace_number(ws).unwrap_or(0),
            pane.map_or(0, |at| at + 1),
        )
    }

    /// `target_index` for input, questions and waits: a lost session refuses them.
    fn live_target(&self, p: &Value, key: &str) -> Result<usize, ApiError> {
        let i = self.target_index(p, key)?;
        let session = &self.sessions.items()[i];
        refuse_lost(session.id, session.lost)?;
        Ok(i)
    }

    /// `live_target` for typing into it: a held (disconnected) SSH session refuses too, since
    /// there is no program to take it.
    fn input_target(&self, p: &Value, key: &str) -> Result<usize, ApiError> {
        let i = self.live_target(p, key)?;
        let session = &self.sessions.items()[i];
        refuse_disconnected(session.id, session.disconnected.is_some())?;
        Ok(i)
    }

    /// The sidebar index of the session `p[key]` names; no target means the caller, else the
    /// focused session.
    pub(crate) fn target_index(&self, p: &Value, key: &str) -> Result<usize, ApiError> {
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

    /// Close the session shown by `view`, as ⌘W does (also a sidebar row's ×, middle-click and
    /// tab menu).
    pub(crate) fn close_view(
        &mut self,
        view: EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(i) = self.sessions.position(|s| s.view.entity_id() == view) {
            self.close_at(i, window, cx);
        }
    }

    fn type_into(&self, index: usize, bytes: &[u8], cx: &mut Context<Self>) {
        let view = self.sessions.items()[index].view.clone();
        view.update(cx, |view, cx| view.type_bytes(bytes, cx));
    }

    /// Name session `index` exactly: empty goes back to the automatic name. (The rename box
    /// names workspaces; a script names terminals.)
    pub(crate) fn rename_at(&mut self, index: usize, name: &str, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        let old = session.name.clone();
        session.name = (!name.is_empty()).then(|| name.to_string());
        let renamed = session.name != old;
        if renamed {
            self.emit_renamed(index);
            self.push_meta(index, cx);
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
            let i = self.live_target(p, "target")?;
            Watch::One(self.sessions.items()[i].id)
        };
        let cond = api::parse_wait(opt_str(p, "for")?, opt_str(p, "text")?)?;
        let now = Instant::now();
        // Too far ahead to represent counts as no limit.
        let deadline = timeout_param(p)?.and_then(|t| now.checked_add(t));
        let sessions = self.statuses();
        let items = self.sessions.items();
        let mut screen = |id: u64| screen_text(items, id, cx);
        let mut wait = Wait::new(
            conn.clone(),
            request.id,
            watch,
            cond,
            &sessions,
            deadline,
            &mut screen,
        );
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
    /// at most `REPLY_FLUSH`, and the background service no longer has the session `ending`
    /// (at most `ENDING_WAIT`); at once when there's nothing to wait for.
    pub(crate) fn quit_after_flush(
        &mut self,
        conns: Vec<Conn>,
        ending: Option<(Arc<DaemonClient>, u64)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let conns: Vec<Conn> = conns.into_iter().filter(|c| !c.peer_gone()).collect();
        if conns.is_empty() && ending.is_none() {
            window.remove_window();
            return;
        }
        self.quitting = true;
        cx.spawn_in(window, async move |_, cx| {
            cx.background_spawn(async move {
                let deadline = Instant::now() + REPLY_FLUSH;
                for conn in conns {
                    conn.flushed(deadline.saturating_duration_since(Instant::now()));
                }
                // Its `Close` goes through the view's queue: quitting first could lose it.
                if let Some((client, id)) = ending
                    && !wait_ended(&client, id, ENDING_WAIT)
                {
                    eprintln!("[fuxx] background service: s{id} wasn't seen to end");
                }
            })
            .await;
            cx.update(|window, _| window.remove_window()).ok();
        })
        .detach();
    }

    /// `session.new`: a session like ⌘N / a preset (a workspace of its own), or with `split` a
    /// pane like ⌘D beside `of`, or with `tab` a tab in `of`'s pane (M11b).
    /// A bad request is an error now; otherwise the reply goes to `conn` once the session is
    /// open (at once without the background service).
    fn api_new(
        &mut self,
        request: &Request,
        conn: &Conn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ApiError> {
        let p = &request.params;
        let ssh = ssh_param(p)?;
        if ssh.is_some() {
            for key in ["folder", "preset", "run"] {
                if opt_str(p, key)?.is_some_and(|v| !v.is_empty()) {
                    return Err(ApiError::bad_request(format!(
                        "ssh and {key} can't be combined"
                    )));
                }
            }
        }
        let preset = match opt_str(p, "preset")? {
            Some(name) => Some(find_preset(&cx.global::<Appearance>().presets, name)?),
            None => None,
        };
        let split = opt_str(p, "split")?.map(split_axis).transpose()?;
        let place = new_place(split, bool_param(p, "tab")?, opt_str(p, "of")?.is_some())?;
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
        let of = match place {
            NewPlace::Split(_) | NewPlace::Tab => Some(self.target_index(p, "of")?),
            NewPlace::Workspace => None,
        };
        let project = folder
            .or_else(|| of.and_then(|i| self.sessions.items()[i].project.clone()))
            .or_else(|| self.active_project());
        if let Some(why) = open_refusal(&self.service, self.restoring) {
            return Err(ApiError::unavailable(format!("no new session: {why}")));
        }
        let previous = self.focus_to_give_back();
        if let Some(target) = ssh {
            let place = match (place, of) {
                (NewPlace::Split(axis), Some(of)) => Place::Split { of, axis },
                (NewPlace::Tab, Some(of)) => Place::Tab { of },
                _ => Place::New,
            };
            let (conn, request_id) = (conn.clone(), request.id);
            let opened: Opened = Box::new(move |this, id, window, cx| {
                let result = this.api_opened(id, previous, None, background, window, cx);
                if result.is_ok()
                    && let (Some(id), Some(name)) = (id, name)
                    && let Some(i) = this.sessions.position(|s| s.id == id)
                {
                    this.rename_at(i, &name, cx);
                }
                conn.send_line(&api::reply(request_id, result));
            });
            self.open_ssh_then(target, place, Some(opened), window, cx);
            return Ok(());
        }
        let at = match (place, of.and_then(|i| self.pane_of(i))) {
            (NewPlace::Split(axis), Some(pane)) => At::Split(axis, pane),
            (NewPlace::Tab, Some(pane)) => At::Tab(pane),
            _ => At::New,
        };
        let launch = Launch {
            saved: SavedSession {
                name,
                project,
                preset,
                daemon_id: None,
                ssh: None,
                color: None,
            },
            run_preset: true,
        };
        let (conn, request_id) = (conn.clone(), request.id);
        let opened: Opened = Box::new(move |this, id, window, cx| {
            let result = this.api_opened(id, previous, run, background, window, cx);
            conn.send_line(&api::reply(request_id, result));
        });
        self.open_session_then(launch, at, Some(opened), window, cx);
        Ok(())
    }

    /// What `session.new --background` gives the focus back to once its session has opened: the
    /// focused tab, a terminal's or a browser tab's (with a page focused no session is active).
    /// By its tab id: the new session's row can push the previous one down.
    pub(crate) fn focus_to_give_back(&self) -> Option<TabId> {
        self.model.focused_tab()
    }

    /// The rest of `session.new` once session `id` opened (`None`: it didn't): type `run`,
    /// and with `background` give the focus back to tab `previous` (`select_tab`, which shows a
    /// browser tab as well as a terminal's).
    pub(crate) fn api_opened(
        &mut self,
        id: Option<u64>,
        previous: Option<TabId>,
        run: Option<String>,
        background: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, ApiError> {
        let index_of = |this: &Self, id: u64| this.sessions.position(|s| s.id == id);
        let Some((id, index)) = id.and_then(|id| Some((id, index_of(self, id)?))) else {
            // Nothing opened (logged).
            return Err(ApiError::refused("could not start a shell"));
        };
        if let Some(run) = run {
            self.type_into(index, format!("{run}\r").as_bytes(), cx);
        }
        if background && let Some(tab) = previous {
            self.select_tab(tab, window, cx);
        }
        Ok(json!({ "id": format!("s{id}") }))
    }
}

/// What `session.list` reports for one session.
struct ListEntry<'a> {
    id: u64,
    number: usize,
    /// Its workspace's place in the sidebar, from 1 (M11b).
    workspace: usize,
    /// Its pane's place in the workspace's reading order, from 1 (M11b).
    pane: usize,
    name: &'a str,
    title: Option<&'a str>,
    status: Status,
    project: Option<&'a Path>,
    preset: Option<&'a str>,
    focused: bool,
    on_screen: bool,
    note: Option<&'a str>,
    progress: Option<&'a (u8, Option<String>)>,
    /// False once the background service was lost: the session no longer updates.
    live: bool,
    /// The host of an SSH session.
    ssh: Option<&'a str>,
    /// Its tab colour, by name (M11); left out when none.
    color: Option<TabColor>,
}

/// Where `session.new` opens a session (`new_place`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NewPlace {
    Workspace,
    Split(Axis),
    Tab,
}

/// Where `session.new` opens a session, from its `split`, `tab` and whether it names `of`: a
/// workspace of its own (⌘N), a pane beside `of` (⌘D), or a tab in `of`'s pane (M11b). `of`
/// without the target's caller or focused default needs one of them; both can't be.
fn new_place(split: Option<Axis>, tab: bool, of: bool) -> Result<NewPlace, ApiError> {
    match (split, tab) {
        (Some(_), true) => Err(ApiError::bad_request("split and tab can't be combined")),
        (Some(axis), false) => Ok(NewPlace::Split(axis)),
        (None, true) => Ok(NewPlace::Tab),
        (None, false) if of => Err(ApiError::bad_request("of needs split or tab")),
        (None, false) => Ok(NewPlace::Workspace),
    }
}

/// The `ssh` param of `session.new` (`{ host, tmux }`): absent is a plain session.
fn ssh_param(p: &Value) -> Result<Option<SshTarget>, ApiError> {
    let ssh = match p.get("ssh") {
        None | Some(Value::Null) => return Ok(None),
        Some(ssh @ Value::Object(_)) => ssh,
        Some(_) => return Err(ApiError::bad_request("ssh must be an object")),
    };
    let host = validate_host(required_str(ssh, "host")?).map_err(ApiError::bad_request)?;
    Ok(Some(SshTarget {
        host,
        tmux: bool_param(ssh, "tmux")?,
        tmux_name: None,
    }))
}

fn list_entry(e: &ListEntry) -> Value {
    let mut entry = json!({
        "id": format!("s{}", e.id),
        "number": e.number,
        "name": e.name,
        "title": e.title,
        "status": e.status.as_str(),
        "project": e.project.map(|p| p.display().to_string()),
        "preset": e.preset,
        "focused": e.focused,
        "on_screen": e.on_screen,
        "note": e.note,
        "progress": e.progress.map(|(percent, label)| json!({ "percent": percent, "label": label })),
        "live": e.live,
        "workspace": e.workspace,
        "pane": e.pane,
        // Every session is a terminal; browser tabs aren't sessions.
        "kind": "terminal",
    });
    if let Some(host) = e.ssh {
        entry["ssh"] = json!(host);
    }
    if let Some(color) = e.color {
        entry["color"] = json!(color.name());
    }
    entry
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
pub(crate) fn opt_str<'a>(p: &'a Value, key: &str) -> Result<Option<&'a str>, ApiError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(ApiError::bad_request(format!("{key} must be text"))),
    }
}

pub(crate) fn required_str<'a>(p: &'a Value, key: &str) -> Result<&'a str, ApiError> {
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

/// The longest note or progress label kept, in characters.
const NOTE_MAX_CHARS: usize = 120;

/// Trimmed text cut to `NOTE_MAX_CHARS` at a character boundary; blank is `None`.
fn clean_note(text: &str) -> Option<String> {
    // Control characters (newlines, tabs) become spaces so the note stays one line.
    let one_line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let capped: String = one_line.trim().chars().take(NOTE_MAX_CHARS).collect();
    let capped = capped.trim_end();
    (!capped.is_empty()).then(|| capped.to_string())
}

/// `session.note`'s `text`: a string sets, null clears, absent or anything else is a bad request.
fn note_param(p: &Value) -> Result<Option<String>, ApiError> {
    match p.get("text") {
        None => Err(ApiError::bad_request("text is missing")),
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(clean_note(s)),
        Some(_) => Err(ApiError::bad_request("text must be text")),
    }
}

/// `session.progress`'s `percent` (0 to 100, clamped, rounded) and `label`; null percent clears.
fn progress_param(p: &Value) -> Result<Option<(u8, Option<String>)>, ApiError> {
    let percent = match p.get("percent") {
        None => return Err(ApiError::bad_request("percent is missing")),
        Some(Value::Null) => return Ok(None),
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(_) => return Err(ApiError::bad_request("percent must be a number")),
    };
    let label = opt_str(p, "label")?.and_then(clean_note);
    Ok(Some((percent.round().clamp(0.0, 100.0) as u8, label)))
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

/// Of the sessions a request reads the screen of (`(id, drawn, lost)`), the ones to wait for
/// (their screen hasn't arrived from the background service yet). A lost one whose screen
/// never arrived can't be read: an error, except for an any-session wait (`any`), which
/// leaves it out (its blank screen never matches).
fn screens_to_wait_for(sessions: &[(u64, bool, bool)], any: bool) -> Result<Vec<u64>, ApiError> {
    let mut wait = Vec::new();
    for &(id, drawn, lost) in sessions {
        match (drawn, lost) {
            (true, _) => {}
            (false, false) => wait.push(id),
            (false, true) if any => {}
            (false, true) => {
                return Err(ApiError::unavailable(format!(
                    "s{id}: the background service was lost before its screen arrived (reopen fuxx)"
                )));
            }
        }
    }
    Ok(wait)
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
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Input, questions and waits need a live session: one whose background service was lost
/// refuses them.
fn refuse_lost(id: u64, lost: bool) -> Result<(), ApiError> {
    if lost {
        return Err(ApiError::unavailable(format!(
            "s{id}: the background service was lost (reopen fuxx)"
        )));
    }
    Ok(())
}

/// Input to a held SSH session (M10b): nothing would take it until it reconnects.
fn refuse_disconnected(id: u64, disconnected: bool) -> Result<(), ApiError> {
    if disconnected {
        return Err(ApiError::refused(format!(
            "s{id}: disconnected \u{2014} reconnect first"
        )));
    }
    Ok(())
}

fn not_waiting(id: u64, status: Status) -> ApiError {
    ApiError::refused(format!("s{id} is not waiting (it is {})", status.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli;

    #[test]
    fn a_screen_not_arrived_yet_is_waited_for_never_read_blank() {
        // (id, drawn, lost)
        assert_eq!(
            screens_to_wait_for(&[(1, true, false)], false).unwrap(),
            Vec::<u64>::new()
        );
        assert_eq!(
            screens_to_wait_for(&[(1, false, false)], false).unwrap(),
            [1]
        );
        // Lost before it arrived: it never will.
        let err = screens_to_wait_for(&[(1, false, true)], false).unwrap_err();
        assert_eq!(err.code, ApiError::unavailable("").code);
        assert!(err.message.contains("s1"), "{}", err.message);
        // Lost after it arrived: its last screen can still be read.
        assert!(
            screens_to_wait_for(&[(1, true, true)], false)
                .unwrap()
                .is_empty()
        );
        // Any session: every one not drawn yet, lost ones left out.
        let any = [
            (1, true, false),
            (2, false, false),
            (3, false, true),
            (4, false, false),
        ];
        assert_eq!(screens_to_wait_for(&any, true).unwrap(), [2, 4]);
    }

    #[test]
    fn list_entry_shape_and_how_ls_prints_it() {
        let e = ListEntry {
            id: 4,
            number: 2,
            workspace: 1,
            pane: 2,
            name: "api",
            title: Some("claude"),
            status: Status::Waiting,
            project: Some(Path::new("/x/api")),
            preset: Some("claude"),
            focused: true,
            on_screen: true,
            note: Some("tests"),
            progress: Some(&(40, Some("build".into()))),
            live: true,
            ssh: None,
            color: None,
        };
        let v = list_entry(&e);
        assert_eq!(
            v,
            json!({
                "id": "s4", "number": 2, "name": "api", "title": "claude", "status": "waiting",
                "project": "/x/api", "preset": "claude", "focused": true, "on_screen": true,
                "note": "tests", "progress": {"percent": 40, "label": "build"},
                "live": true, "workspace": 1, "pane": 2, "kind": "terminal",
            })
        );
        assert_eq!(
            cli::render("ls", &json!([v]), false),
            "s4  #2*  waiting  api  [/x/api]  (claude)  40% build · tests"
        );
        let bare = list_entry(&ListEntry {
            title: None,
            project: None,
            preset: None,
            focused: false,
            on_screen: false,
            note: None,
            progress: None,
            ..e
        });
        assert_eq!(
            (
                &bare["title"],
                &bare["project"],
                &bare["preset"],
                &bare["note"],
                &bare["progress"]
            ),
            (
                &Value::Null,
                &Value::Null,
                &Value::Null,
                &Value::Null,
                &Value::Null
            )
        );
    }

    #[test]
    fn list_entry_names_its_tab_color_only_when_set() {
        let e = ListEntry {
            workspace: 1,
            pane: 1,
            id: 4,
            number: 2,
            name: "api",
            title: None,
            status: Status::Idle,
            project: None,
            preset: None,
            focused: false,
            on_screen: false,
            note: None,
            progress: None,
            live: true,
            ssh: None,
            color: Some(TabColor::Orange),
        };
        assert_eq!(list_entry(&e)["color"], json!("orange"));
        let plain = list_entry(&ListEntry { color: None, ..e });
        assert!(plain.get("color").is_none(), "{plain}");
    }

    #[test]
    fn api_new_ssh_validates_host() {
        assert_eq!(ssh_param(&json!({})), Ok(None));
        assert_eq!(ssh_param(&json!({"ssh": null})), Ok(None));
        let t = ssh_param(&json!({"ssh": {"host": " me@dev ", "tmux": true}}))
            .unwrap()
            .unwrap();
        assert_eq!(
            (t.host.as_str(), t.tmux, t.tmux_name),
            ("me@dev", true, None)
        );
        let t = ssh_param(&json!({"ssh": {"host": "dev"}}))
            .unwrap()
            .unwrap();
        assert!(!t.tmux);
        for bad in ["", "-oProxyCommand=x", "a b"] {
            let e = ssh_param(&json!({"ssh": {"host": bad}})).unwrap_err();
            assert_eq!(e.code, "bad_request", "{bad}");
            assert_eq!(e.message, validate_host(bad).unwrap_err(), "{bad}");
        }
        for odd in [
            json!({"ssh": "dev"}),
            json!({"ssh": {}}),
            json!({"ssh": {"host": 3}}),
            json!({"ssh": {"host": "d", "tmux": "yes"}}),
        ] {
            assert_eq!(ssh_param(&odd).unwrap_err().code, "bad_request", "{odd}");
        }
    }

    #[test]
    fn list_shows_ssh_host_only_for_ssh() {
        let mut e = ListEntry {
            workspace: 1,
            pane: 1,
            id: 4,
            number: 2,
            name: "api",
            title: None,
            status: Status::Idle,
            project: None,
            preset: None,
            focused: false,
            on_screen: false,
            note: None,
            progress: None,
            live: true,
            ssh: None,
            color: None,
        };
        assert!(list_entry(&e).get("ssh").is_none());
        e.ssh = Some("me@dev");
        assert_eq!(list_entry(&e)["ssh"], json!("me@dev"));
    }

    #[test]
    fn a_lost_session_is_listed_as_not_live_and_refuses_input() {
        let e = ListEntry {
            workspace: 1,
            pane: 1,
            id: 4,
            number: 2,
            name: "api",
            title: None,
            status: Status::Waiting,
            project: None,
            preset: None,
            focused: false,
            on_screen: false,
            note: None,
            progress: None,
            live: false,
            ssh: None,
            color: None,
        };
        let v = list_entry(&e);
        assert_eq!(v["live"], json!(false));
        assert_eq!(cli::render("ls", &json!([v]), false), "s4  #2  lost  api");
        assert_eq!(refuse_lost(4, false), Ok(()));
        let e = refuse_lost(4, true).unwrap_err();
        assert_eq!(e.code, "unavailable");
        assert!(e.message.contains("s4"), "{}", e.message);
    }

    #[test]
    fn input_to_a_disconnected_ssh_session_is_refused() {
        assert_eq!(refuse_disconnected(4, false), Ok(()));
        let e = refuse_disconnected(4, true).unwrap_err();
        assert_eq!(e.code, "refused");
        assert_eq!(e.message, "s4: disconnected \u{2014} reconnect first");
    }

    #[test]
    fn note_text_is_trimmed_capped_or_cleared() {
        assert_eq!(
            note_param(&json!({"text": "  hi  "})),
            Ok(Some("hi".into()))
        );
        assert_eq!(note_param(&json!({"text": "   "})), Ok(None));
        assert_eq!(note_param(&json!({"text": ""})), Ok(None));
        assert_eq!(note_param(&json!({"text": null})), Ok(None));
        for bad in [json!({}), json!({"text": 5}), json!({"text": true})] {
            assert_eq!(note_param(&bad).unwrap_err().code, "bad_request", "{bad}");
        }
        let long = "é".repeat(300);
        let got = note_param(&json!({ "text": long })).unwrap().unwrap();
        assert_eq!(got.chars().count(), NOTE_MAX_CHARS);
        assert_eq!(clean_note(&"a".repeat(120)).unwrap().len(), 120);
        // One line in the sidebar: control characters become spaces.
        assert_eq!(clean_note("build\nok\tnow"), Some("build ok now".into()));
        assert_eq!(clean_note("\n\r\t"), None);
    }

    #[test]
    fn progress_percent_is_clamped_rounded_or_cleared() {
        let pr = |v: Value| progress_param(&json!({ "percent": v }));
        assert_eq!(pr(json!(40)), Ok(Some((40, None))));
        assert_eq!(pr(json!(0)), Ok(Some((0, None))));
        assert_eq!(pr(json!(250)), Ok(Some((100, None))));
        assert_eq!(pr(json!(-5)), Ok(Some((0, None))));
        assert_eq!(pr(json!(40.6)), Ok(Some((41, None))));
        assert_eq!(pr(Value::Null), Ok(None));
        for bad in [json!("40"), json!(true), json!([1])] {
            assert_eq!(pr(bad.clone()).unwrap_err().code, "bad_request", "{bad}");
        }
        assert_eq!(progress_param(&json!({})).unwrap_err().code, "bad_request");
        assert_eq!(
            progress_param(&json!({"percent": 5, "label": " build "})),
            Ok(Some((5, Some("build".into()))))
        );
        assert_eq!(
            progress_param(&json!({"percent": 5, "label": " "})),
            Ok(Some((5, None)))
        );
        assert_eq!(
            progress_param(&json!({"percent": 5, "label": 3}))
                .unwrap_err()
                .code,
            "bad_request"
        );
        assert_eq!(
            progress_param(&json!({"percent": null, "label": "x"})),
            Ok(None)
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

    #[test]
    fn a_new_session_goes_in_a_workspace_a_pane_or_a_tab() {
        use super::{NewPlace, new_place};
        // As before: a workspace of its own, or a split (with or without of).
        assert_eq!(new_place(None, false, false), Ok(NewPlace::Workspace));
        assert_eq!(
            new_place(Some(Axis::Row), false, true),
            Ok(NewPlace::Split(Axis::Row))
        );
        assert_eq!(
            new_place(Some(Axis::Column), false, false),
            Ok(NewPlace::Split(Axis::Column))
        );
        // New: a tab in of's pane (or the caller's / focused one's).
        assert_eq!(new_place(None, true, true), Ok(NewPlace::Tab));
        assert_eq!(new_place(None, true, false), Ok(NewPlace::Tab));
        // Not both; and of needs one of them.
        let err = |r: Result<NewPlace, ApiError>| r.unwrap_err().message;
        assert_eq!(
            err(new_place(Some(Axis::Row), true, false)),
            "split and tab can't be combined"
        );
        assert_eq!(err(new_place(None, false, true)), "of needs split or tab");
    }
}
