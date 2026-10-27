//! M10a: the workspace's side of the background service. Who runs the sessions (`Service`):
//! connecting at startup, a lost connection (the sessions stay; one reconnect, which shows
//! the same service's live sessions again), the Quit item's count, and "Quit fuxx and End All
//! Sessions". Opening, restoring and closing sessions live in `workspace`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::{Context, EntityId, Window, prelude::*};

use crate::daemon;
use crate::daemon_client::DaemonClient;
use crate::mux_proto;
use crate::session_list::SessionList;
use crate::terminal_view::TerminalView;
use crate::workspace::{Workspace, home_dir};

/// Who runs the sessions (M10a).
pub(crate) enum Service {
    /// This process (persistence off, or the background service couldn't start).
    Off,
    /// Connecting to the background service at startup; no session opens yet.
    Starting,
    On(Arc<DaemonClient>),
    /// The connection to the background service (process `pid`) was lost. The sessions stay,
    /// marked; no new one starts (the ids are the service's).
    Lost {
        pid: u32,
    },
}

impl Service {
    pub(crate) fn client(&self) -> Option<Arc<DaemonClient>> {
        match self {
            Service::On(client) => Some(client.clone()),
            _ => None,
        }
    }
}

/// How long after losing the background service fuxx tries to reach it again.
const RECONNECT_AFTER: Duration = Duration::from_millis(500);

const LOST_NOTICE: &str = "Lost the background service: these sessions no longer update and new \
                           ones can't start. Reopen fuxx to restore them.";

impl Workspace {
    /// Connect to the background service (starting it if needed) off the UI thread, then
    /// reopen the sessions. If it can't be reached, this launch runs them itself.
    pub(crate) fn connect_service(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let socket = daemon_socket();
        let connecting = cx.background_spawn(async move {
            let socket = socket.ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "$HOME is unset")
            })?;
            DaemonClient::connect_or_spawn(&socket, &std::env::current_exe()?)
        });
        cx.spawn_in(window, async move |this, cx| {
            let connected = connecting.await;
            this.update_in(cx, |this, window, cx| {
                this.service_started(connected, window, cx)
            })
            .ok();
        })
        .detach();
    }

    fn service_started(
        &mut self,
        connected: std::io::Result<Arc<DaemonClient>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match connected {
            Ok(client) => {
                eprintln!(
                    "[fuxx] background service: connected (pid {})",
                    client.pid()
                );
                self.service = Service::On(client);
            }
            Err(err) => {
                eprintln!(
                    "[fuxx] background service: unavailable ({err}); sessions end when fuxx quits this time"
                );
                self.notice = Some(
                    format!(
                        "Background service unavailable, so sessions end when fuxx quits this time ({err})."
                    )
                    .into(),
                );
                self.service = Service::Off;
            }
        }
        self.restore(window, cx);
    }

    /// The Quit item says how many sessions keep running (persistence on); the menu bar is
    /// rebuilt only when that number changes.
    pub(crate) fn sync_quit_title(&mut self, cx: &mut Context<Self>) {
        if !self.persistent {
            return;
        }
        let n = match self.service {
            Service::On(_) => self.sessions.items().iter().filter(|s| !s.lost).count(),
            _ => 0,
        };
        if n != self.quit_count {
            self.quit_count = n;
            crate::set_menus(cx, Some(n));
        }
    }

    /// Session `index`'s view lost the background service. Not an exit: the session stays (and
    /// stays in the sessions file, so a reopen can restore it). The first view to notice speaks
    /// for the connection: the service is marked lost and fuxx tries once to reach it again.
    pub(crate) fn daemon_lost(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.sessions.get_mut(index).filter(|s| !s.lost) else {
            return;
        };
        session.lost = true;
        if let Service::On(client) = &self.service {
            let pid = client.pid();
            eprintln!(
                "[fuxx] background service: lost the connection (pid {pid}); keeping the sessions, trying again"
            );
            self.service = Service::Lost { pid };
            self.notice = Some(LOST_NOTICE.into());
            self.reconnect(pid, window, cx);
        }
        self.sync_quit_title(cx);
        cx.notify();
    }

    /// Try once to reach the background service again (connect only: a new one wouldn't have
    /// these sessions).
    fn reconnect(&mut self, pid: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(socket) = daemon_socket() else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(RECONNECT_AFTER).await;
            let found = cx
                .background_spawn(async move {
                    let client = DaemonClient::connect(&socket)?;
                    let live = client.list()?;
                    Ok::<_, std::io::Error>((client, live))
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.reconnected(pid, found, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// The same service (process `pid`) answers: show its live sessions again and go on with
    /// it. Another one, or none: the sessions stay lost.
    fn reconnected(
        &mut self,
        pid: u32,
        found: std::io::Result<(Arc<DaemonClient>, Vec<mux_proto::SessionInfo>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.service, Service::Lost { pid: lost } if lost == pid) {
            return;
        }
        match found {
            Ok((client, live)) if client.pid() == pid => {
                let live: Vec<u64> = live.iter().filter(|s| s.alive).map(|s| s.id).collect();
                eprintln!("[fuxx] background service: reconnected (pid {pid})");
                self.reattach_lost(&client, &live, window, cx);
                self.service = Service::On(client);
                let ended = self.sessions.items().iter().filter(|s| s.lost).count();
                self.notice = (ended > 0).then(|| {
                    format!(
                        "Reconnected to the background service; {ended} session(s) ended meanwhile."
                    )
                    .into()
                });
            }
            Ok((client, _)) => eprintln!(
                "[fuxx] background service: another one runs now (pid {}); the sessions stay lost",
                client.pid()
            ),
            Err(err) => eprintln!("[fuxx] background service: still unreachable: {err}"),
        }
        self.sync_quit_title(cx);
        cx.notify();
    }

    /// Give every lost session the service still runs (`live`) a new view of it, in place.
    fn reattach_lost(
        &mut self,
        client: &Arc<DaemonClient>,
        live: &[u64],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for index in 0..self.sessions.items().len() {
            let session = &self.sessions.items()[index];
            if !session.lost || !live.contains(&session.id) {
                continue;
            }
            let (id, old) = (session.id, session.view.entity_id());
            let view = match TerminalView::attach_daemon(window, cx, client.clone(), id) {
                Ok(view) => view,
                Err(err) => {
                    eprintln!("[fuxx] session: could not show s{id} again: {err}");
                    continue;
                }
            };
            let new = view.entity_id();
            let swap = |v: EntityId| if v == old { new } else { v };
            self.layouts = self.layouts.iter().map(|t| t.map(&swap)).collect();
            if self.renaming.as_ref().is_some_and(|(v, _)| *v == old) {
                self.renaming = None;
            }
            if self.dragging.as_ref().is_some_and(|(v, _)| *v == old) {
                self.dragging = None;
            }
            let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
            if let Some(session) = self.sessions.get_mut(index) {
                session.view = view;
                session._subscription = subscription;
                session.lost = false;
            }
        }
        self.focus_active(window, cx);
    }

    /// "Quit fuxx and End All Sessions": end every session (in the background service, once
    /// it has them), save the empty list as closing the last session does, then quit.
    pub fn end_all_and_quit(&mut self, cx: &mut Context<Self>) {
        let ending: Vec<u64> = self
            .sessions
            .items()
            .iter()
            .filter(|s| !s.lost)
            .map(|s| s.id)
            .collect();
        // Dropping the views ends Local sessions and detaches the service's.
        self.sessions = SessionList::new();
        self.layouts.clear();
        self.collapsed.clear();
        self.renaming = None;
        self.dragging = None;
        self.restoring = false; // even mid-startup, the list saved is the empty one
        self.save();
        cx.notify();
        match self.service.client() {
            Some(client) if !ending.is_empty() => {
                cx.spawn(async move |_, cx| {
                    cx.background_spawn(async move {
                        for id in ending {
                            client.close(id);
                        }
                        // Answered after the Closes (one connection, in order): they arrived.
                        if let Err(err) = client.list() {
                            eprintln!("[fuxx] background service: ending sessions: {err}");
                        }
                    })
                    .await;
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
            _ => cx.quit(),
        }
    }
}

/// The background service's socket (`FUXX_DAEMON_SOCKET`, else under `$HOME`).
pub(crate) fn daemon_socket() -> Option<PathBuf> {
    let env_socket = std::env::var("FUXX_DAEMON_SOCKET").ok();
    daemon::client_socket_path(env_socket.as_deref(), home_dir().as_deref())
}

/// Wait (at most `within`, polling) until the background service no longer lists session
/// `id`: its `Close`, sent through the view's queue, has arrived. False on timeout or error.
pub(crate) fn wait_ended(client: &DaemonClient, id: u64, within: Duration) -> bool {
    let deadline = std::time::Instant::now() + within;
    loop {
        match client.list() {
            Ok(sessions) if !sessions.iter().any(|s| s.id == id) => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::wait_ended;

    #[test]
    fn waiting_for_a_closed_session_to_be_gone() {
        use crate::daemon_client::DaemonClient;
        use crate::daemon_client::tests::{daemon, sh_create};
        use std::time::{Duration, Instant};
        let (_d, path) = daemon("ws-ended");
        let client = DaemonClient::connect(&path).unwrap();
        let kept = client.create(sh_create(40, 5)).unwrap();
        let closed = client.create(sh_create(40, 5)).unwrap();
        // Still running: false once the time is up.
        let start = Instant::now();
        assert!(!wait_ended(&client, kept, Duration::from_millis(300)));
        assert!(start.elapsed() >= Duration::from_millis(300));
        client.close(closed);
        assert!(wait_ended(&client, closed, Duration::from_secs(5)));
        assert!(client.list().unwrap().iter().any(|s| s.id == kept));
    }
}
