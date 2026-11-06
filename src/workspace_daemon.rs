//! M10a: the workspace's side of the background service. Who runs the sessions (`Service`):
//! connecting at startup, reopening (the service's live sessions in their saved places, the
//! rest restored; screens fetched as they're shown), a lost connection (the sessions stay; one
//! reconnect, which shows the same service's live sessions again), the Quit item's count, and
//! "Quit fuxx and End All Sessions". Opening and closing sessions live in `workspace`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{Context, Window, prelude::*};

use crate::adopt::{self, LiveInfo, Placement, Probe, Slot};
use crate::appearance::Appearance;
use crate::daemon;
use crate::daemon_client::{self, DaemonClient};
use crate::groups;
use crate::layout_model::Model;
use crate::mux_proto::{self, Create, Progress, SetMeta};
use crate::restore::{self, RestoreMode, SavedSession, SavedState};
use crate::session_list::SessionList;
use crate::ssh_target::SshTarget;
use crate::status::Status;
use crate::terminal::PumpQueue;
use crate::terminal_view::TerminalView;
use crate::theme::Theme;
use crate::workspace::{Session, Workspace, created, home_dir, read_saved};
use crate::workspace_browser::Browsers;
use crate::workspace_ssh::{check_restored, ssh_program, ssh_refusal};

/// The sidebar notice when persistence is off but `live` sessions still run in a background
/// service.
pub(crate) fn leftover_notice(live: usize) -> String {
    let sessions = if live == 1 {
        "1 session still runs".to_string()
    } else {
        format!("{live} sessions still run")
    };
    format!(
        "{sessions} in the background service \u{2014} turn Preferences \u{2192} Sessions \u{2192} Keep running after quit back on, or run `fuxx daemon stop --force`."
    )
}

/// The sidebar notice when persistence is on but the background service couldn't be reached
/// (`err`); `held`: saved sessions it may still run weren't started again.
pub(crate) fn unavailable_notice(err: &str, held: bool) -> String {
    let notice = format!(
        "Background service unavailable, so sessions end when fuxx quits this time ({err})."
    );
    if !held {
        return notice;
    }
    format!(
        "{notice} Sessions it may still run weren't started again \u{2014} reopen fuxx to try again, or run `fuxx daemon stop --force`."
    )
}

/// The sidebar notice when persistence is off and a background service is there but didn't
/// answer, so the saved sessions it ran weren't started again.
pub(crate) const UNANSWERED_NOTICE: &str = "A background service is running but didn't answer, \
     so sessions it may still run weren't started again \u{2014} turn Preferences \u{2192} \
     Sessions \u{2192} Keep running after quit back on, or run `fuxx daemon stop --force`.";

/// Who runs the sessions (M10a).
pub(crate) enum Service {
    /// This process (persistence off, or the background service couldn't start).
    Off,
    /// Connecting to the background service at startup; no session opens yet.
    Starting,
    On(Arc<DaemonClient>),
    /// The connection to the background service (process `pid`, random id `instance`) was
    /// lost. The sessions stay, marked; no new one starts (the ids are the service's).
    Lost {
        pid: u32,
        instance: Option<u64>,
    },
}

/// Why a new session can't open now, if it can't: while connecting, after the service was
/// lost (a new one would reuse the lost sessions' numbers), and while the service's sessions
/// are being reopened (their layouts are put back at the end).
pub(crate) fn open_refusal(service: &Service, restoring: bool) -> Option<&'static str> {
    match service {
        Service::Off => None,
        Service::Starting => Some("the background service is starting"),
        Service::Lost { .. } => Some("the background service was lost (reopen fuxx)"),
        Service::On(_) if restoring => Some("fuxx is reopening its sessions"),
        Service::On(_) => None,
    }
}

impl Service {
    pub(crate) fn client(&self) -> Option<Arc<DaemonClient>> {
        match self {
            Service::On(client) => Some(client.clone()),
            _ => None,
        }
    }

    /// The random id of the service the sessions belong to (also once it was lost).
    pub(crate) fn instance(&self) -> Option<u64> {
        match self {
            Service::On(client) => client.instance(),
            Service::Lost { instance, .. } => *instance,
            Service::Off | Service::Starting => None,
        }
    }
}

/// After reopening, the sessions not on screen fetch their screens from this long on, one at
/// a time this far apart, so the window is up first.
const ATTACH_REST_AFTER: Duration = Duration::from_secs(1);
const ATTACH_STAGGER: Duration = Duration::from_millis(100);

/// At most this long, quitting waits for what the views queued for the background service.
const QUIT_FLUSH: Duration = Duration::from_millis(150);

/// At most this long, starting with persistence off waits for a background service that is
/// there to say which sessions it still runs.
const PROBE_WAIT: Duration = Duration::from_millis(500);

/// How long after a failed List at startup fuxx asks once more.
const RELIST_AFTER: Duration = Duration::from_millis(500);

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

    /// Persistence is off: look (connect only, never start one) for a background service still
    /// holding sessions from an earlier run, and say so in the sidebar. Nothing is adopted; the
    /// restore leaves what it still runs to it (`adopt::without_service`). Nothing listening
    /// is found at once; otherwise this waits at most `PROBE_WAIT`.
    pub(crate) fn probe_leftover_service(&mut self) -> Probe {
        let probe = daemon_socket().map_or(Probe::Absent, |s| probe_service(&s, PROBE_WAIT));
        match &probe {
            Probe::Found { live, .. } if !live.is_empty() => {
                let notice = leftover_notice(live.len());
                eprintln!("[fuxx] background service: {notice}");
                self.notice = Some(notice.into());
            }
            Probe::NoAnswer => {
                eprintln!("[fuxx] background service: one is there but didn't answer the probe");
            }
            _ => {}
        }
        probe
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
                service_ready(&client, &cx.global::<Appearance>().theme);
                self.service = Service::On(client.clone());
                self.reopen(client, window, cx);
            }
            Err(err) => {
                eprintln!(
                    "[fuxx] background service: unavailable ({err}); sessions end when fuxx quits this time"
                );
                self.service = Service::Off;
                // It may still run the saved sessions: those aren't started a second time.
                let held = self.restore(&Probe::NoAnswer, window, cx);
                self.notice = Some(unavailable_notice(&err.to_string(), held).into());
            }
        }
    }

    /// Reopen with the background service: list its live sessions, place them by the sessions
    /// file (`adopt::place`), start again the saved ones no longer running (per
    /// `session-restore`), then show them all. Live sessions are shown even with restore off.
    pub(crate) fn reopen(
        &mut self,
        client: Arc<DaemonClient>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let listing = client.clone();
            let mut listed = cx.background_spawn(async move { listing.list() }).await;
            if let Err(err) = &listed {
                eprintln!(
                    "[fuxx] background service: could not list its sessions ({err}); trying again"
                );
                cx.background_executor().timer(RELIST_AFTER).await;
                let listing = client.clone();
                listed = cx.background_spawn(async move { listing.list() }).await;
            }
            let Ok((placement, saved, creates)) =
                this.update(cx, |this, cx| this.placement(&client, listed, cx))
            else {
                return;
            };
            // In order, one at a time, as the sessions were saved. An SSH one reads its ssh
            // settings first, off the UI thread (M10b).
            let mut ids = Vec::with_capacity(creates.len());
            let mut refused = None;
            for create in creates {
                ids.push(match create {
                    Some((mut create, ssh)) => {
                        let is_ssh = ssh.is_some();
                        if let Some(target) = ssh {
                            let saved = SavedSession {
                                ssh: Some(target.clone()),
                                ..Default::default()
                            };
                            let program =
                                cx.background_spawn(async move { ssh_program(&target, None) });
                            create.program = Some(program.await);
                            // Again, now the login shell's environment has been read for it.
                            if let Ok(env) = this.update(cx, |this, _| this.create_env(&saved)) {
                                create.env = env;
                            }
                        }
                        let answer = client.create_in_background(create).await;
                        if is_ssh && refused.is_none() {
                            refused = ssh_refusal(&answer);
                        }
                        created(answer)
                    }
                    None => None,
                });
            }
            this.update_in(cx, |this, window, cx| {
                this.adopt(&client, placement, &saved, &ids, window, cx);
                if let Some(why) = refused {
                    this.ssh_failed(&why, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Where every session goes (the placement, and the sessions file it places them by), and
    /// per slot the `Create` of one to start again (with its SSH target: its program is filled
    /// in once read).
    #[allow(clippy::type_complexity)]
    fn placement(
        &mut self,
        client: &DaemonClient,
        listed: std::io::Result<Vec<mux_proto::SessionInfo>>,
        cx: &Context<Self>,
    ) -> (
        Placement,
        SavedState,
        Vec<Option<(Create, Option<SshTarget>)>>,
    ) {
        if let Err(err) = &listed {
            eprintln!(
                "[fuxx] background service: could not list its sessions ({err}); not reopening them, and keeping the sessions file for the next launch"
            );
        }
        let appearance = cx.global::<Appearance>();
        let (live, mode, hold) = listed_sessions(listed, appearance.restore);
        self.hold_saving |= hold;
        let saved = read_saved().unwrap_or_default();
        let mut placement =
            adopt::place(&saved, &live, client.instance(), mode, &appearance.presets);
        // A saved host is checked as a typed one is: a refused one isn't started.
        let mut refused = Vec::new();
        for (i, slot) in placement.slots.iter_mut().enumerate() {
            if let Slot::Restore(launch) = slot
                && let Err(why) = check_restored(launch)
            {
                self.note_ssh_failed(&why);
                refused.push(i);
            }
        }
        let placement = placement.without(&refused);
        let creates = placement
            .slots
            .iter()
            .map(|slot| match slot {
                Slot::Restore(launch) => {
                    Some((self.create_for(launch, None), launch.saved.ssh.clone()))
                }
                Slot::Live(_) => None,
            })
            .collect();
        (placement, saved, creates)
    }

    /// Show the placed sessions in their saved tabs (`saved`): live ones without their screens
    /// yet (fetched when shown, or in the background soon after), restarted ones (`ids`, per
    /// slot) at once. Slots that can't be shown are left out (their panes' neighbours take
    /// their space); with nothing to show (no terminal, and no workspace of pages), one plain
    /// session opens.
    fn adopt(
        &mut self,
        client: &Arc<DaemonClient>,
        placement: Placement,
        saved: &SavedState,
        ids: &[Option<u64>],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut views = Vec::new();
        let mut gone = Vec::new();
        for (i, slot) in placement.slots.iter().enumerate() {
            let view = match (slot, ids.get(i).copied().flatten()) {
                (Slot::Live(a), _) => {
                    TerminalView::adopt_daemon(cx, client.clone(), a.id).map(|v| (a.id, v))
                }
                (Slot::Restore(_), Some(id)) => {
                    TerminalView::attach_daemon(cx, client.clone(), id)
                        .map(|v| (id, v))
                        .inspect_err(|_| {
                            // Not shown, so not left running unseen.
                            let client = client.clone();
                            cx.background_spawn(async move { client.close(id) })
                                .detach();
                        })
                }
                (Slot::Restore(_), None) => Err("it didn't start".to_string()),
            };
            match view {
                Ok(view) => views.push(view),
                Err(err) => {
                    eprintln!("[fuxx] restore: a session couldn't be shown: {err}");
                    gone.push(i);
                }
            }
        }
        // The service went away meanwhile: what's shown is missing what couldn't start, so the
        // sessions file stays as it is for the next launch.
        if client.is_closed() || !matches!(self.service, Service::On(_)) {
            eprintln!(
                "[fuxx] background service: lost while reopening; keeping the sessions file for the next launch"
            );
            self.hold_saving = true;
        }
        let placement = placement.without(&gone);
        let shown: Vec<u64> = views.iter().map(|(id, _)| *id).collect();
        let model = adopt::workspaces(saved, &placement, &shown);
        self.collapsed = groups::collapsed_keys(&placement.collapsed, &placement.collapsed_hosts);
        for (slot, (id, view)) in placement.slots.into_iter().zip(views) {
            let session = self.session_for(slot, id, view, window, cx);
            let at = self.sessions.items().len();
            self.sessions.insert(at, session);
        }
        if !restore::shows_anything(&model, !self.sessions.is_empty()) {
            eprintln!("[fuxx] restore: no session to show, opening a new one");
            self.open_first(window, cx);
            return;
        }
        self.set_model(model);
        // The normal route, so the focused workspace's group opens (and its panes attach as
        // they are drawn).
        self.activate(window, cx);
        self.finish_restoring(window, cx);
        self.attach_rest_later(cx);
    }

    /// The sidebar entry for a placed slot: a live one with the service's facts (its ring
    /// shows before its screen arrives), a restarted one fresh.
    fn session_for(
        &mut self,
        slot: Slot,
        id: u64,
        view: gpui::Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Session {
        let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
        let (saved, title, status, note, progress) = match slot {
            Slot::Live(a) => (a.saved, a.title, a.status, a.note, a.progress),
            Slot::Restore(launch) => (launch.saved, None, Status::Idle, None, None),
        };
        Session {
            id,
            view,
            status,
            status_since: Instant::now(),
            pop: None,
            question: None,
            title: title.map(Into::into),
            name: saved.name,
            project: saved.project,
            preset: saved.preset,
            note,
            progress,
            lost: false,
            ssh: saved.ssh,
            disconnected: None,
            retries: 0,
            opened_at: Instant::now(),
            reconnected: false,
            _subscription: subscription,
        }
    }

    /// After a second, the sessions not shown yet fetch their screens, one at a time.
    fn attach_rest_later(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ATTACH_REST_AFTER).await;
            while let Ok(true) = this.update(cx, |this, cx| this.attach_next(cx)) {
                cx.background_executor().timer(ATTACH_STAGGER).await;
            }
        })
        .detach();
    }

    /// Fetch the first session's screen not fetched yet; false when every one has it.
    fn attach_next(&self, cx: &Context<Self>) -> bool {
        self.sessions
            .items()
            .iter()
            .any(|s| s.view.read(cx).attach())
    }

    /// Quitting (⌘Q, the red button, ⌥⌘Q): give each view's queue to the background service
    /// time to go out (GPUI waits up to its shutdown timeout).
    pub(crate) fn flush_on_quit(&mut self, cx: &mut Context<Self>) -> gpui::Task<()> {
        let queues: Vec<PumpQueue> = self
            .sessions
            .items()
            .iter()
            .filter_map(|s| s.view.read(cx).pump_queue())
            .collect();
        let client = self.service.client();
        cx.background_spawn(async move {
            if client.is_some() {
                flush_pumps(&queues, client.as_deref(), QUIT_FLUSH);
            }
        })
    }

    /// Tell the background service session `index`'s name, note and progress, so a reopen
    /// shows them (the service's facts win over the sessions file). Local: nothing.
    pub(crate) fn push_meta(&self, index: usize, cx: &Context<Self>) {
        let Some(s) = self.sessions.items().get(index).filter(|s| !s.lost) else {
            return;
        };
        s.view.read(cx).set_meta(SetMeta {
            id: s.id,
            name: Some(s.name.clone()),
            project: None,
            preset: None,
            note: Some(s.note.clone()),
            progress: Some(
                s.progress
                    .clone()
                    .map(|(percent, label)| Progress { percent, label }),
            ),
        });
    }

    /// The Quit item says how many sessions keep running (persistence on); the menu bar is
    /// rebuilt only when that number changes.
    pub(crate) fn sync_quit_title(&mut self, cx: &mut Context<Self>) {
        // While restoring, once at the end (`finish_restoring`).
        if !self.persistent || self.restoring {
            return;
        }
        let n = match self.service {
            // A held SSH session's session has ended.
            Service::On(_) => {
                let running = |s: &&Session| !s.lost && s.disconnected.is_none();
                self.sessions.items().iter().filter(running).count()
            }
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
        // Only a view of the current connection speaks for it: a late one from a connection
        // already replaced (after a reconnect) loses just its own session.
        let current = match &self.service {
            Service::On(client) => session.view.read(cx).uses_client(client),
            _ => false,
        };
        if current && let Service::On(client) = &self.service {
            let lost = (client.pid(), client.instance());
            eprintln!(
                "[fuxx] background service: lost the connection (pid {}); keeping the sessions, trying again",
                lost.0
            );
            self.service = Service::Lost {
                pid: lost.0,
                instance: lost.1,
            };
            self.notice = Some(LOST_NOTICE.into());
            self.reconnect(lost, window, cx);
        }
        self.sync_quit_title(cx);
        cx.notify();
    }

    /// Try once to reach the background service again (connect only: a new one wouldn't have
    /// these sessions).
    fn reconnect(&mut self, lost: (u32, Option<u64>), window: &mut Window, cx: &mut Context<Self>) {
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
                this.reconnected(lost, found, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// The same service (`lost`: pid and instance; `same_service`) answers: show its live
    /// sessions again and go on with it. Another one, or none: the sessions stay lost.
    fn reconnected(
        &mut self,
        lost: (u32, Option<u64>),
        found: std::io::Result<(Arc<DaemonClient>, Vec<mux_proto::SessionInfo>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.service, Service::Lost { pid, instance } if (pid, instance) == lost) {
            return;
        }
        match found {
            Ok((client, live)) if same_service(lost, (client.pid(), client.instance())) => {
                let live: Vec<u64> = live.iter().filter(|s| s.alive).map(|s| s.id).collect();
                eprintln!("[fuxx] background service: reconnected (pid {})", lost.0);
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
            let id = session.id;
            let view = match TerminalView::attach_daemon(cx, client.clone(), id) {
                Ok(view) => view,
                Err(err) => {
                    eprintln!("[fuxx] session: could not show s{id} again: {err}");
                    continue;
                }
            };
            // Its tab names it by id, so it stays where it is.
            let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
            if let Some(session) = self.sessions.get_mut(index) {
                session.view = view;
                session._subscription = subscription;
                session.lost = false;
            }
            // A rename, note or progress while it was lost reached only the window.
            self.push_meta(index, cx);
        }
        self.focus_active(window, cx);
    }

    /// "Quit fuxx and End All Sessions": end every session (in the background service, once
    /// it has them), save the empty list as closing the last session does, then quit.
    pub fn end_all_and_quit(&mut self, cx: &mut Context<Self>) {
        if self.quitting {
            eprintln!("[fuxx] already quitting: waiting for the sessions to end");
            return;
        }
        self.quitting = true;
        let lost = self.sessions.items().iter().filter(|s| s.lost).count();
        if lost > 0 {
            eprintln!(
                "[fuxx] background service: {lost} lost session(s) not ended (no connection to them)"
            );
        }
        let ending: Vec<u64> = self
            .sessions
            .items()
            .iter()
            .filter(|s| !s.lost)
            .map(|s| s.id)
            .collect();
        // Dropping the views ends Local sessions and detaches the service's.
        self.sessions = SessionList::new();
        self.model = Model::new();
        self.collapsed.clear();
        self.renaming = None;
        self.pane_tab_menu = None;
        self.address_box = None;
        // The browser tabs go with their workspaces (their pages leave the window on the next
        // frame), and the popped-out windows with them.
        let closed = self.browser_tabs.drain().map(|(_, view)| view);
        self.closed_browser_tabs.extend(closed);
        self.browser_titles.clear();
        self.browsers = Browsers::new();
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

/// Wait (at most `within`) until every pump (`queues`) has handed its commands to the
/// connection, then until the service has read them: a List, answered after them on the one
/// connection. So a rename or note just before quitting isn't lost.
pub(crate) fn flush_pumps(queues: &[PumpQueue], client: Option<&DaemonClient>, within: Duration) {
    let deadline = std::time::Instant::now() + within;
    while !queues.iter().all(PumpQueue::is_empty) {
        if std::time::Instant::now() >= deadline {
            eprintln!("[fuxx] background service: quitting before everything was sent");
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    if let Some(client) = client
        && let Err(err) = client.list()
    {
        eprintln!("[fuxx] background service: quitting unconfirmed: {err}");
    }
}

/// Whether a service found again (`found`: pid, instance) is the one lost (`lost`). With
/// instance ids, they decide (a pid can be reused); between older daemons, the pid.
pub(crate) fn same_service(lost: (u32, Option<u64>), found: (u32, Option<u64>)) -> bool {
    match (lost.1, found.1) {
        (None, None) => lost.0 == found.0,
        (lost, found) => lost == found,
    }
}

/// What reopening goes on: the live sessions listed, the restore mode to use, and whether
/// saving must wait. A failed List shows nothing, starts nothing again (that could double
/// sessions still running), and keeps the sessions file as it is for the next launch.
pub(crate) fn listed_sessions(
    list: std::io::Result<Vec<mux_proto::SessionInfo>>,
    mode: RestoreMode,
) -> (Vec<LiveInfo>, RestoreMode, bool) {
    match list {
        Ok(list) => (
            list.iter().filter_map(LiveInfo::from_list).collect(),
            mode,
            false,
        ),
        Err(_) => (Vec::new(), RestoreMode::Off, true),
    }
}

/// Look (connect only, never start one) at the background service at `socket`, waiting at
/// most `within` for its answer.
/// Nothing listening (or a service exiting) is `Absent`; a service that doesn't answer in time
/// or can't be talked to is `NoAnswer`.
pub(crate) fn probe_service(socket: &Path, within: Duration) -> Probe {
    let socket = socket.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    let probing = std::thread::Builder::new()
        .name("fuxx-probe".into())
        .spawn(move || {
            let found = match DaemonClient::connect(&socket) {
                Err(e) if daemon_client::respawnable(&e) => Probe::Absent,
                Err(_) => Probe::NoAnswer,
                Ok(client) => match client.list() {
                    Ok(list) => Probe::Found {
                        instance: client.instance(),
                        live: list.iter().filter(|s| s.alive).map(|s| s.id).collect(),
                    },
                    Err(_) => Probe::NoAnswer,
                },
            };
            // The look took too long and nobody waits any more: dropped.
            let _ = tx.send(found);
        });
    if probing.is_err() {
        return Probe::NoAnswer;
    }
    rx.recv_timeout(within).unwrap_or(Probe::NoAnswer)
}

/// Right after connecting, before any session is created: the user's theme, so even the
/// first sessions of a fresh service answer colour queries with it.
pub(crate) fn service_ready(client: &DaemonClient, theme: &Theme) {
    client.set_theme(theme);
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
    use super::{
        Service, leftover_notice, listed_sessions, open_refusal, same_service, wait_ended,
    };
    use crate::restore::RestoreMode;

    #[test]
    fn the_same_service_is_known_by_its_instance_not_its_pid() {
        use crate::daemon_client::DaemonClient;
        use crate::daemon_client::tests::daemon;
        let (_a, a_path) = daemon("ws-same-a");
        let (_b, b_path) = daemon("ws-same-b");
        let a = DaemonClient::connect(&a_path).unwrap();
        let b = DaemonClient::connect(&b_path).unwrap();
        let id = |c: &DaemonClient| (c.pid(), c.instance());
        // In-process daemons share the pid, as a reused pid would.
        assert!(!same_service(id(&a), id(&b)));
        assert!(same_service(
            id(&a),
            id(&DaemonClient::connect(&a_path).unwrap())
        ));
        // Older daemons (no instance): the pid.
        assert!(same_service((5, None), (5, None)));
        assert!(!same_service((5, None), (6, None)));
        // One side has an instance, the other not: different daemons.
        assert!(!same_service((5, Some(1)), (5, None)));
    }

    #[test]
    fn quitting_waits_until_what_the_views_sent_has_reached_the_service() {
        use super::flush_pumps;
        use crate::daemon_client::DaemonClient;
        use crate::daemon_client::tests::{daemon, sh_create};
        use crate::mux_proto::SetMeta;
        use crate::terminal::TerminalCore;
        use crate::theme::Theme;
        use std::time::Duration;
        let (_d, path) = daemon("ws-flush");
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, _rx) = futures::channel::mpsc::unbounded();
        let core =
            TerminalCore::create_daemon(client.clone(), sh_create(40, 5), tx, Theme::default())
                .unwrap();
        let id = core.daemon_id().unwrap();
        let queues = [core.pump_queue().unwrap()];
        for round in 0..10 {
            for n in 0..300 {
                core.set_meta(SetMeta {
                    id,
                    name: Some(Some(format!("r{round}-{n}"))),
                    project: None,
                    preset: None,
                    note: None,
                    progress: None,
                });
            }
            flush_pumps(&queues, Some(&client), Duration::from_secs(5));
            assert!(queues[0].is_empty());
            assert_eq!(client.list().unwrap()[0].name, format!("r{round}-299"));
        }
    }

    #[test]
    fn a_failed_list_shows_nothing_restarts_nothing_and_holds_the_file() {
        let err = std::io::Error::new(std::io::ErrorKind::TimedOut, "no answer");
        let (live, mode, hold) = listed_sessions(Err(err), RestoreMode::Agents);
        assert!(live.is_empty());
        assert_eq!(mode, RestoreMode::Off);
        assert!(hold, "the sessions file is kept for the next launch");
        let (live, mode, hold) = listed_sessions(Ok(vec![]), RestoreMode::Agents);
        assert!(live.is_empty() && !hold);
        assert_eq!(mode, RestoreMode::Agents);
    }

    #[test]
    fn new_sessions_wait_for_the_service_and_its_restore() {
        use crate::daemon_client::DaemonClient;
        use crate::daemon_client::tests::daemon;
        assert_eq!(open_refusal(&Service::Off, false), None);
        // A Local restore opens its sessions itself, synchronously.
        assert_eq!(open_refusal(&Service::Off, true), None);
        let starting = open_refusal(&Service::Starting, true).unwrap();
        assert!(starting.contains("starting"), "{starting}");
        let lost = open_refusal(
            &Service::Lost {
                pid: 9,
                instance: None,
            },
            false,
        )
        .unwrap();
        assert!(lost.contains("lost"), "{lost}");
        let (_d, path) = daemon("ws-refusal");
        let on = Service::On(DaemonClient::connect(&path).unwrap());
        assert_eq!(open_refusal(&on, false), None);
        let reopening = open_refusal(&on, true).unwrap();
        assert!(reopening.contains("reopening"), "{reopening}");
    }

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

    #[test]
    fn the_leftover_notice_counts_sessions_and_names_both_ways_out() {
        let one = leftover_notice(1);
        assert!(one.starts_with("1 session still runs in the background service"));
        let many = leftover_notice(3);
        assert!(many.starts_with("3 sessions still run in the background service"));
        for n in [&one, &many] {
            assert!(n.contains("Preferences \u{2192} Sessions \u{2192} Keep running after quit"));
            assert!(n.contains("fuxx daemon stop --force"));
        }
    }

    #[test]
    fn starting_without_the_service_leaves_its_live_sessions_to_it() {
        use super::probe_service;
        use crate::adopt::{Probe, without_service};
        use crate::daemon_client::DaemonClient;
        use crate::daemon_client::tests::{daemon, sh_create, temp_sock};
        use crate::restore::{SavedSession, SavedState, VERSION};
        use crate::split_tree::Node;
        use std::time::{Duration, Instant};
        let (_d, path) = daemon("ws-probe");
        let client = DaemonClient::connect(&path).unwrap();
        let held = client.create(sh_create(40, 5)).unwrap();
        let s = |name: &str, daemon_id: Option<u64>| SavedSession {
            name: Some(name.into()),
            daemon_id,
            ..Default::default()
        };
        let saved = SavedState {
            version: VERSION,
            active: Some(0),
            collapsed: Vec::new(),
            collapsed_hosts: Vec::new(),
            sessions: vec![s("held", Some(held)), s("gone", Some(held + 1))],
            layouts: vec![Node::Leaf(0), Node::Leaf(1)],
            daemon_instance: client.instance(),
            ..SavedState::default()
        };
        let probe = probe_service(&path, Duration::from_secs(5));
        assert_eq!(
            probe,
            Probe::Found {
                instance: client.instance(),
                live: vec![held]
            }
        );
        let (state, hold) = without_service(saved.clone(), &probe);
        assert!(hold, "the sessions file is kept");
        let restarted: Vec<_> = state.sessions.iter().map(|s| s.name.clone()).collect();
        assert_eq!(
            restarted,
            [Some("gone".to_string())],
            "held is not restarted"
        );
        // Another service's file: today's behaviour.
        let other = SavedState {
            daemon_instance: client.instance().map(|i| i ^ 1),
            ..saved
        };
        assert_eq!(without_service(other.clone(), &probe), (other, false));
        // Nothing listening: found at once.
        let start = Instant::now();
        let none = temp_sock("ws-probe-none");
        assert_eq!(probe_service(&none, Duration::from_secs(5)), Probe::Absent);
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn a_service_that_does_not_answer_in_time_is_no_answer() {
        use super::probe_service;
        use crate::adopt::Probe;
        use crate::daemon_client::tests::temp_sock;
        use std::time::{Duration, Instant};
        // Listening, but never says HelloOk.
        let path = temp_sock("ws-probe-mute");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let start = Instant::now();
        assert_eq!(
            probe_service(&path, Duration::from_millis(200)),
            Probe::NoAnswer
        );
        assert!(start.elapsed() < Duration::from_secs(1), "bounded");
    }

    #[test]
    fn a_fresh_service_knows_the_theme_before_the_first_session_is_created() {
        use super::service_ready;
        use crate::daemon_client::DaemonClient;
        use crate::daemon_client::tests::{daemon, sh_create};
        use crate::theme::{Theme, parse_color};
        let (running, path) = daemon("ws-theme");
        let client = DaemonClient::connect(&path).unwrap();
        let mut theme = Theme::default();
        theme.background = parse_color("#123456").unwrap();
        service_ready(&client, &theme);
        // Same connection, so the daemon handled what came before this Create.
        client.create(sh_create(40, 5)).unwrap();
        assert_eq!(running.theme(), Some(theme));
    }

    #[test]
    fn the_notices_say_when_sessions_were_left_to_an_unreachable_service() {
        use super::{UNANSWERED_NOTICE, unavailable_notice};
        let plain = unavailable_notice("timed out", false);
        assert_eq!(
            plain,
            "Background service unavailable, so sessions end when fuxx quits this time (timed out)."
        );
        let held = unavailable_notice("timed out", true);
        assert!(held.starts_with(&plain), "{held}");
        assert!(held.contains("weren't started again"), "{held}");
        assert!(held.contains("fuxx daemon stop --force"), "{held}");
        assert!(UNANSWERED_NOTICE.contains("didn't answer"));
        assert!(UNANSWERED_NOTICE.contains("weren't started again"));
        assert!(UNANSWERED_NOTICE.contains("fuxx daemon stop --force"));
    }
}
