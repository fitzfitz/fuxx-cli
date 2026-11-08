//! M10b: SSH sessions in the workspace. Opening one (the host checked, then its ssh settings
//! read off the UI thread, and `ssh` run as the pane's program), holding the pane when the
//! connection fails (ssh exits 255) with a reconnect line, and reconnecting it in place (Enter,
//! or with tmux a timer). Grouping by host is in `groups`, saving in `restore`.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::{Context, Entity, EntityId, Focusable, Task, Window, prelude::*};

use crate::api_events::Event;
use crate::host_picker::{HostPicker, HostPickerEvent};
use crate::layout_model::TabKind;
use crate::restore::{Launch, SavedSession};
use crate::split_tree::Axis;
use crate::ssh_target::{self, Effective, SshTarget};
use crate::status::Status;
use crate::terminal_view::TerminalView;
use crate::workspace::{At, Opened, Overlay, Session, Workspace, home_dir};
use crate::workspace_daemon::open_refusal;
use crate::{ssh_control, ssh_hosts};

/// At most this long each for `ssh -G` and the shared-connection check before an SSH session
/// opens.
const SSH_CHECK: Duration = Duration::from_secs(2);

/// An SSH connection that was up this long (printing from the first output on) starts the
/// automatic reconnect waits over, and its drop is news again.
const RETRIES_FORGOTTEN: Duration = Duration::from_secs(60);

/// An SSH session whose ssh fails (non-zero) sooner than this after it opened is held, so the
/// error can be read (e.g. tmux missing on the host).
const EARLY_EXIT: Duration = Duration::from_secs(10);

/// At most this long for the login shell to print its environment (once per run).
const LOGIN_ENV_CAP: Duration = Duration::from_secs(3);

/// Written into the screen (only) before the reconnect line, so it isn't drawn in the remote
/// program's state: plain colours, out of a full-screen program's alternate screen, and the
/// whole screen as the scroll region (the cursor saved around it, since setting the region
/// moves it home).
const SCREEN_RESET: &str = "\x1b[0m\x1b[?1049l\x1b7\x1b[r\x1b8";

/// A held SSH session (M10b): its screen stays, with the reconnect line.
pub(crate) struct Disconnected {
    /// With tmux, the automatic reconnect; dropped (the pane closed, or it reconnected),
    /// it never fires.
    _timer: Option<Task<()>>,
    /// Why it is held (shown again if a reconnect can't start).
    held: Held,
    /// A reconnect is under way (so Enter and the timer don't both start one).
    reconnecting: bool,
    /// That reconnect is the timer's: whatever has the focus keeps it.
    auto: bool,
}

/// Where an SSH session opens: a workspace of its own, a pane beside (Row) or under (Column)
/// session `of`'s, a tab in session `of`'s pane, or in place of session `index` (a reconnect:
/// same tab, name and group).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    New,
    Split { of: usize, axis: Axis },
    Tab { of: usize },
    Replace(usize),
}

/// What a session's program ending does to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitAction {
    /// It is taken away, as always.
    Close,
    /// An SSH session whose connection failed, or that failed early: the pane stays for a
    /// reconnect.
    Hold(Held),
}

/// Why an SSH session's pane is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Held {
    /// ssh exited 255: the connection failed or dropped.
    Dropped,
    /// ssh ended with this code soon after it opened (the remote side's error).
    Ended(u32),
}

/// For an SSH session (`lasted`: since it opened): `Hold` when ssh exited 255 ("connection
/// failed"), or failed with another code within `EARLY_EXIT` (its error stays readable). A
/// logout, a failure after a session that ran a while, or an unknown code closes it as any
/// session.
pub fn on_exit(ssh: Option<&SshTarget>, code: Option<u32>, lasted: Duration) -> ExitAction {
    match code {
        _ if ssh.is_none() => ExitAction::Close,
        Some(255) => ExitAction::Hold(Held::Dropped),
        Some(code) if code != 0 && lasted < EARLY_EXIT => ExitAction::Hold(Held::Ended(code)),
        _ => ExitAction::Close,
    }
}

/// The wait before automatic reconnect `attempt` (0-based, tmux sessions): 3 s, doubling
/// each time, at most 60 s.
pub fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs((3u64 << attempt.min(5)).min(60))
}

/// The line a held SSH session shows, on a line of its own; `next`: the seconds until the
/// automatic reconnect (tmux). Control characters in `host` are left out (one that passed
/// `validate_host` has none), so it can't drive the screen.
pub fn disconnect_banner(host: &str, next: Option<u64>) -> String {
    let host: String = host.chars().filter(|c| !c.is_control()).collect();
    let line = match next {
        Some(secs) => format!("Disconnected from {host} — reconnecting in {secs}s, or press Enter"),
        None => format!("Disconnected from {host} — press Enter to reconnect"),
    };
    format!("\r\n{line}\r\n")
}

/// The line a held pane shows: the reconnect line for a drop (`next` as there), or for an early
/// failure `ssh ended (exit N) — press Enter to reconnect` (never an automatic try).
pub fn held_line(host: &str, held: Held, next: Option<u64>) -> String {
    match held {
        Held::Dropped => disconnect_banner(host, next),
        Held::Ended(code) => {
            format!("\r\nssh ended (exit {code}) \u{2014} press Enter to reconnect\r\n")
        }
    }
}

/// What a held pane shows: the screen reset (`SCREEN_RESET`), then the reconnect line.
pub fn held_screen(host: &str, held: Held, next: Option<u64>) -> String {
    format!("{SCREEN_RESET}{}", held_line(host, held, next))
}

/// Whether a connection was really up, from how long ago it first printed (`None`: never):
/// for `RETRIES_FORGOTTEN`. A try stuck connecting prints only ssh's error, as it gives up.
pub fn was_up(up_for: Option<Duration>) -> bool {
    up_for.is_some_and(|d| d >= RETRIES_FORGOTTEN)
}

/// Whether a connection's drop is news (the usual done notification, per the M4 rules): the
/// first of a run. A reconnect that drops again before it was up (`was_up`) is the same drop
/// going on.
pub fn drop_is_news(reconnected: bool, was_up: bool) -> bool {
    !reconnected || was_up
}

/// The variables in `env -0` output, in order. Each is `NAME=value` ended by a NUL (a value may
/// hold newlines); text an rc file printed before them is dropped, as is anything after the
/// last NUL and a name that isn't one. The login shell's own `PWD`, `OLDPWD`, `SHLVL` and `_`
/// are left out: they describe that shell, not the session.
pub fn parse_env(text: &str) -> Vec<(String, String)> {
    let Some((records, _unended)) = text.rsplit_once('\0') else {
        return Vec::new();
    };
    records
        .split('\0')
        .filter_map(|record| {
            let (name, value) = record.split_once('=')?;
            // A name never holds a newline: before one is what an rc file printed.
            let name = name.rsplit('\n').next().unwrap_or(name);
            let valid = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            let own = matches!(name, "PWD" | "OLDPWD" | "SHLVL" | "_");
            (valid && !own).then(|| (name.to_string(), value.to_string()))
        })
        .collect()
}

/// The environment `shell` gives an interactive login (`shell -ilc 'command env -0'`, no
/// stdin), as your terminal's sessions get it. `None` if it fails, prints none, or takes longer
/// than `timeout` (it is killed then). Blocks, so off the UI thread.
pub fn capture_login_env(shell: &OsStr, timeout: Duration) -> Option<Vec<(String, String)>> {
    let mut command = std::process::Command::new(shell);
    command.args(["-ilc", "command env -0"]);
    let (status, text) = ssh_hosts::run_capped(&mut command, timeout)?;
    let env = parse_env(&text);
    (status.success() && !env.is_empty()).then_some(env)
}

/// Your login shell's environment, read once per run (the first call blocks up to
/// `LOGIN_ENV_CAP`, so off the UI thread): SSH sessions start from it, so ssh finds what it
/// would in your terminal (`PATH` for a ProxyCommand's tools, `SSH_AUTH_SOCK`, …). `None` if it
/// couldn't be read: fuxx's own is used.
pub(crate) fn login_env() -> Option<&'static [(String, String)]> {
    LOGIN_ENV
        .get_or_init(|| {
            let shell = portable_pty::CommandBuilder::new_default_prog().get_shell();
            let env = capture_login_env(OsStr::new(&shell), LOGIN_ENV_CAP);
            if env.is_none() {
                eprintln!("[fuxx] ssh: could not read {shell}'s environment; using fuxx's own");
            }
            env
        })
        .as_deref()
}

/// `login_env` if it has been read already; never blocks.
pub(crate) fn login_env_read() -> Option<&'static [(String, String)]> {
    LOGIN_ENV.get().and_then(Option::as_deref)
}

static LOGIN_ENV: OnceLock<Option<Vec<(String, String)>>> = OnceLock::new();

/// The target a session opens with. With tmux it has its own tmux session name: a new one for
/// a new session (`fresh`), else the one it has (a reconnect or restore comes back to it),
/// made if it has none. Without tmux, none.
pub fn session_target(target: SshTarget, fresh: bool) -> SshTarget {
    let tmux_name = match target.tmux_name {
        _ if !target.tmux => None,
        Some(name) if !fresh => Some(name),
        _ => Some(new_tmux_name(&target.host)),
    };
    SshTarget {
        tmux_name,
        ..target
    }
}

/// `<host>-<6 random hex digits>`: tmux-safe, and at most 32 characters (the host part is
/// cut), as the name after `fuxx-` must be.
fn new_tmux_name(host: &str) -> String {
    use std::hash::BuildHasher;
    // Keyed from the OS's randomness, and new keys for every `RandomState`.
    let random = std::collections::hash_map::RandomState::new()
        .hash_one((std::time::SystemTime::now(), std::process::id()));
    let host: String = ssh_target::tmux_session_name(host)
        .chars()
        .take(25)
        .collect();
    format!("{host}-{:06x}", random & 0xff_ffff)
}

/// A saved SSH session about to be started again: its host checked and trimmed as a typed
/// one is, and its tmux name kept (made if missing). Err: the host is refused, so it isn't
/// started. A local session passes as it is.
pub(crate) fn check_restored(launch: &mut Launch) -> Result<(), String> {
    let Some(target) = &launch.saved.ssh else {
        return Ok(());
    };
    let host = ssh_target::validate_host(&target.host)
        .map_err(|why| format!("saved host {:?}: {why}", target.host))?;
    let checked = SshTarget {
        host,
        ..target.clone()
    };
    launch.saved.ssh = Some(session_target(checked, false));
    Ok(())
}

/// `launches` (each with its place in the sessions file) without the SSH ones whose saved host
/// is refused (`check_restored`), and why each of those was.
pub(crate) fn checked_launches(
    launches: Vec<(usize, Launch)>,
) -> (Vec<(usize, Launch)>, Vec<String>) {
    let mut refused = Vec::new();
    let kept = launches
        .into_iter()
        .filter_map(|(i, mut launch)| match check_restored(&mut launch) {
            Ok(()) => Some((i, launch)),
            Err(why) => {
                refused.push(why);
                None
            }
        })
        .collect();
    (kept, refused)
}

/// `ssh_argv` with the session's own tmux name (the host for one without).
pub(crate) fn argv_for(
    target: &SshTarget,
    effective: &Effective,
    control: Option<&Path>,
) -> Vec<String> {
    let name = target.tmux_name.as_deref().unwrap_or(&target.host);
    ssh_target::ssh_argv(target, name, effective, control)
}

/// The argv an SSH session runs: connection reuse and keepalive only where the host's
/// `ssh -G` shows them unset, and the shared connection checked first when fuxx's own is
/// used. With `recent`, the host is recorded in that recent-hosts file. The login shell's
/// environment is read first, the first time (`login_env`), for the session to start from.
/// Blocks (ssh runs), so off the UI thread.
pub(crate) fn ssh_program(target: &SshTarget, recent: Option<&Path>) -> Vec<String> {
    login_env();
    // Without ssh's answer nothing is known to be unset: ssh runs as your config has it.
    let effective = ssh_hosts::resolve(&target.host, SSH_CHECK).unwrap_or(Effective {
        has_control_master: true,
        has_keepalive: true,
        ..Effective::default()
    });
    let control = if effective.has_control_master {
        None
    } else {
        ssh_control::control_dir()
    };
    if let Some(dir) = &control {
        ssh_control::ensure_healthy(&target.host, dir, SSH_CHECK);
    }
    if let Some(path) = recent {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Err(err) = ssh_hosts::record_recent(path, target, now) {
            eprintln!("[fuxx] ssh: could not save {}: {err}", path.display());
        }
    }
    argv_for(target, &effective, control.as_deref())
}

/// Why the background service didn't create an SSH session (e.g. a version-1 service, which
/// can't run a program directly), for the sidebar notice; `None` if it did or didn't answer.
pub(crate) fn ssh_refusal(
    answer: &Result<std::io::Result<u64>, futures::channel::oneshot::Canceled>,
) -> Option<String> {
    match answer {
        Ok(Err(err)) => Some(err.to_string()),
        _ => None,
    }
}

impl Workspace {
    /// A reconnected SSH session takes the place of the held one (by view `old`): its tab (so
    /// its pane and workspace), its name, note and progress. Scripts see the old one close and
    /// the new one open in the same place. False if `old` is gone (its tab was closed
    /// meanwhile).
    pub(crate) fn replace_session(
        &mut self,
        old: EntityId,
        session_id: u64,
        view: Entity<TerminalView>,
        saved: SavedSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.sessions.position(|s| s.view.entity_id() == old) else {
            return false;
        };
        let old_id = self.sessions.items()[index].id;
        // Its tab shows the new terminal (the old one ends as its view is dropped below). With
        // no tab, or the new one shown already (never, by the model's rules), it isn't placed,
        // so the caller ends it rather than leave it running unseen.
        let swapped = self
            .model
            .terminal_tab(old_id)
            .and_then(|tab| self.model.set_kind(tab, TabKind::Terminal(session_id)));
        if swapped.is_none() {
            eprintln!("[fuxx] ssh: s{old_id}'s tab couldn't take s{session_id}");
            return false;
        }
        let old_session = &self.sessions.items()[index];
        // Enter asked for it, or the pane had the focus: the new view takes it. A timer's try
        // leaves it where it is (another pane, the host picker, a rename box).
        let take_focus = old_session.disconnected.as_ref().is_none_or(|d| !d.auto)
            || old_session.view.focus_handle(cx).is_focused(window);
        self.emit(&Event::Closed, Some(index));
        self.check_waits(&[old_id], cx);
        let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
        if take_focus {
            view.focus_handle(cx).focus(window, cx);
        }
        let Some(session) = self.sessions.get_mut(index) else {
            return false;
        };
        // Dropping the held session's view (and with it its timer).
        *session = Session {
            id: session_id,
            view,
            status: Status::Idle,
            status_since: Instant::now(),
            pop: None,
            question: None,
            title: None,
            name: saved.name,
            project: saved.project,
            preset: saved.preset,
            note: session.note.take(),
            progress: session.progress.take(),
            activity: std::mem::take(&mut session.activity),
            lost: false,
            ssh: saved.ssh,
            disconnected: None,
            retries: session.retries,
            opened_at: Instant::now(),
            reconnected: true,
            _subscription: subscription,
        };
        self.sync();
        let index = self.index_of(session_id).unwrap_or(index);
        self.emit(&Event::Opened, Some(index));
        // The background service's new session hasn't had the note and progress.
        self.push_meta(index, cx);
        self.note_active(cx);
        true
    }

    /// Open an SSH session to `target` (M10b) at `place`. The host is checked first; then, off
    /// the UI thread, its ssh settings are read, the shared connection is checked and the host
    /// is recorded as recent; then the pane runs `ssh` itself (no project; grouped by host). A
    /// bad host, or a session the background service refuses, is said in the sidebar notice.
    pub(crate) fn open_ssh(
        &mut self,
        target: SshTarget,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_ssh_then(target, place, None, window, cx);
    }

    /// `open_ssh`, then `then` with the new session's id (`None`: it didn't start).
    pub(crate) fn open_ssh_then(
        &mut self,
        target: SshTarget,
        place: Place,
        then: Option<Opened>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let not_opened = |this: &mut Self, then: Option<Opened>, window: &mut Window, cx| {
            if let Some(then) = then {
                then(this, None, window, cx);
            }
        };
        // A new session gets its own tmux name; a reconnect keeps the one it has.
        let fresh = !matches!(place, Place::Replace(_));
        let target = match ssh_target::validate_host(&target.host) {
            Ok(host) => session_target(SshTarget { host, ..target }, fresh),
            Err(why) => {
                self.ssh_failed(&why, cx);
                return not_opened(self, then, window, cx);
            }
        };
        let (at, name) = match place {
            Place::New => (At::New, None),
            Place::Split { of, axis } => (
                self.pane_of(of).map_or(At::New, |p| At::Split(axis, p)),
                None,
            ),
            Place::Tab { of } => (self.pane_of(of).map_or(At::New, At::Tab), None),
            Place::Replace(index) => match self.sessions.items().get(index) {
                Some(old) => (At::Replace(old.view.entity_id()), old.name.clone()),
                None => return not_opened(self, then, window, cx),
            },
        };
        if let Some(why) = open_refusal(&self.service, self.restoring) {
            eprintln!("[fuxx] session: not started, {why}");
            return not_opened(self, then, window, cx);
        }
        let launch = Launch {
            saved: SavedSession {
                name,
                ssh: Some(target.clone()),
                ..Default::default()
            },
            run_preset: false,
        };
        let recent = home_dir().map(|home| ssh_hosts::recent_path(&home));
        let program = cx.background_spawn(async move { ssh_program(&target, recent.as_deref()) });
        cx.spawn_in(window, async move |this, cx| {
            let program = program.await;
            this.update_in(cx, |this, window, cx| {
                this.open_now(launch, at, Some(program), then, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// File → New SSH Workspace…: open the host picker with the recent hosts and those in
    /// `~/.ssh/config`, and put the typing there. Already open: back to its box.
    pub(crate) fn open_host_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = &self.host_picker {
            picker.focus_handle(cx).focus(window, cx);
            return;
        }
        let (recent, hosts) = match home_dir() {
            Some(home) => (
                ssh_hosts::load_recent(&ssh_hosts::recent_path(&home)),
                ssh_hosts::config_hosts(&home.join(".ssh").join("config"), &home),
            ),
            None => (Vec::new(), Vec::new()),
        };
        self.close_overlays(Overlay::HostPicker);
        let picker = cx.new(|cx| HostPicker::new(recent, hosts, window, cx));
        cx.subscribe_in(&picker, window, Self::on_host_picker_event)
            .detach();
        picker.focus_handle(cx).focus(window, cx);
        self.host_picker = Some(picker);
        cx.notify();
    }

    /// The picker chose a host (a new session to it, as its own layout) or was closed; either
    /// way it goes and the typing returns to the terminal.
    fn on_host_picker_event(
        &mut self,
        picker: &Entity<HostPicker>,
        event: &HostPickerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A picker already closed can still report losing focus: ignore it.
        if self.host_picker.as_ref() != Some(picker) {
            return;
        }
        self.host_picker = None;
        if let HostPickerEvent::Open(target) = event {
            self.open_ssh(target.clone(), Place::New, window, cx);
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    /// An SSH session couldn't open (`why`): logged, and said at the foot of the sidebar.
    pub(crate) fn ssh_failed(&mut self, why: &str, cx: &mut Context<Self>) {
        self.note_ssh_failed(why);
        cx.notify();
    }

    /// `ssh_failed` for the caller to redraw.
    pub(crate) fn note_ssh_failed(&mut self, why: &str) {
        eprintln!("[fuxx] ssh: session not started: {why}");
        self.notice = Some(format!("SSH session not started: {why}").into());
    }

    /// The active session's SSH target and index, if it is an SSH session.
    pub(crate) fn active_ssh(&self) -> Option<(usize, SshTarget)> {
        let index = self.sessions.active_index()?;
        let target = self.sessions.items()[index].ssh.clone()?;
        Some((index, target))
    }

    /// SSH session `index`'s connection failed (ssh exited 255), or ssh failed early (`held`):
    /// it stays, with its screen and the reconnect line, and its ring shows done. Only the
    /// first drop of a run notifies (`drop_is_news`). After a drop, with tmux, it reconnects by
    /// itself after a wait that grows with each try in a row (the remote programs are there to
    /// come back to).
    pub(crate) fn hold(
        &mut self,
        index: usize,
        held: Held,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        let Some(host) = session.ssh.as_ref().map(|t| t.host.clone()) else {
            return;
        };
        // Up a while (printing since its first output) starts the waits over.
        let up = was_up(session.view.read(cx).first_output().map(|t| t.elapsed()));
        if up {
            session.retries = 0;
        }
        let news = drop_is_news(session.reconnected, up);
        match held {
            Held::Dropped => eprintln!("[fuxx] ssh: s{} disconnected from {host}", session.id),
            Held::Ended(code) => {
                eprintln!(
                    "[fuxx] ssh: s{} to {host} ended early (exit {code})",
                    session.id
                )
            }
        }
        self.await_reconnect(index, held, window, cx);
        self.set_status(index, Status::Done, news, cx);
        // Its background-service session has ended: one fewer keeps running.
        self.sync_quit_title(cx);
    }

    /// Show held session `index`'s reconnect line and, after a drop with tmux, start the wait
    /// for the next automatic try (its length from the tries so far).
    fn await_reconnect(
        &mut self,
        index: usize,
        held: Held,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        let Some(target) = session.ssh.clone() else {
            return;
        };
        let wait = (target.tmux && held == Held::Dropped).then(|| reconnect_delay(session.retries));
        let view = session.view.entity_id();
        let timer = wait.map(|wait| {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(wait).await;
                this.update_in(cx, |this, window, cx| {
                    if let Some(i) = this.sessions.position(|s| s.view.entity_id() == view) {
                        this.reconnect_ssh(i, true, window, cx);
                    }
                })
                .ok();
            })
        });
        // Replacing an earlier wait drops (cancels) it.
        session.disconnected = Some(Disconnected {
            _timer: timer,
            held,
            reconnecting: false,
            auto: false,
        });
        let text = held_screen(&target.host, held, wait.map(|w| w.as_secs()));
        session.view.update(cx, |view, cx| view.hold(&text, cx));
    }

    /// Reconnect held SSH session `index` in place (Enter, or the tmux timer when `auto`):
    /// same pane, name, group, host, option and tmux session, on a fresh screen. If it can't
    /// start, the pane stays held: the line is shown again and, with tmux, the next try waits.
    pub(crate) fn reconnect_ssh(
        &mut self,
        index: usize,
        auto: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        let Some(target) = session.ssh.clone() else {
            return;
        };
        let Some(held) = session.disconnected.as_mut().filter(|d| !d.reconnecting) else {
            return;
        };
        held.reconnecting = true;
        held.auto = auto;
        let reason = held.held;
        session.retries = if auto {
            session.retries.saturating_add(1)
        } else {
            0
        };
        let view = session.view.entity_id();
        let failed: Opened = Box::new(move |this, id, window, cx| {
            if id.is_none()
                && let Some(i) = this.sessions.position(|s| s.view.entity_id() == view)
            {
                this.await_reconnect(i, reason, window, cx);
            }
        });
        self.open_ssh_then(target, Place::Replace(index), Some(failed), window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExitAction, Held, RETRIES_FORGOTTEN, argv_for, capture_login_env, check_restored,
        checked_launches, disconnect_banner, drop_is_news, held_line, held_screen, on_exit,
        parse_env, reconnect_delay, session_target, was_up,
    };
    use crate::restore::{Launch, SavedSession};
    use crate::ssh_target::{Effective, SshTarget};
    use crate::terminal::{TermDims, TerminalCore};
    use crate::theme::Theme;
    use alacritty_terminal::term::TermMode;
    use std::time::{Duration, Instant};

    fn tmux(host: &str, name: Option<&str>) -> SshTarget {
        SshTarget {
            host: host.into(),
            tmux: true,
            tmux_name: name.map(String::from),
        }
    }

    fn ssh_launch(host: &str) -> Launch {
        Launch {
            saved: SavedSession {
                ssh: Some(tmux(host, None)),
                ..Default::default()
            },
            run_preset: false,
        }
    }

    #[test]
    fn on_exit_holds_drops_and_early_failures() {
        let ssh = SshTarget {
            host: "devbox".into(),
            tmux: false,
            tmux_name: None,
        };
        let (soon, later) = (Duration::from_secs(1), Duration::from_secs(30));
        // ssh's "connection failed" code: the pane stays, however long it ran.
        assert_eq!(
            on_exit(Some(&ssh), Some(255), later),
            ExitAction::Hold(Held::Dropped)
        );
        assert_eq!(
            on_exit(Some(&ssh), Some(255), soon),
            ExitAction::Hold(Held::Dropped)
        );
        // A failure soon after it opened (tmux missing on the host: 127): held, so the
        // remote error can be read.
        assert_eq!(
            on_exit(Some(&ssh), Some(127), soon),
            ExitAction::Hold(Held::Ended(127))
        );
        assert_eq!(
            on_exit(Some(&ssh), Some(1), soon),
            ExitAction::Hold(Held::Ended(1))
        );
        // A logout, a session that ran a while, or a code not known: closed as today.
        assert_eq!(on_exit(Some(&ssh), Some(0), soon), ExitAction::Close);
        assert_eq!(on_exit(Some(&ssh), Some(1), later), ExitAction::Close);
        assert_eq!(on_exit(Some(&ssh), Some(130), later), ExitAction::Close);
        assert_eq!(on_exit(Some(&ssh), None, soon), ExitAction::Close);
        // A local session closes as always.
        assert_eq!(on_exit(None, Some(255), soon), ExitAction::Close);
        assert_eq!(on_exit(None, Some(127), soon), ExitAction::Close);
    }

    #[test]
    fn reconnect_delays() {
        let secs: Vec<u64> = (0..8).map(|n| reconnect_delay(n).as_secs()).collect();
        assert_eq!(secs, [3, 6, 12, 24, 48, 60, 60, 60]);
        assert_eq!(reconnect_delay(u32::MAX), Duration::from_secs(60));
    }

    #[test]
    fn banner_texts() {
        // Control characters never reach the screen, whatever the host.
        assert!(!disconnect_banner("dev\u{1b}[2Jbox", None)[2..].contains('\u{1b}'));
        assert_eq!(
            disconnect_banner("devbox", None),
            "\r\nDisconnected from devbox \u{2014} press Enter to reconnect\r\n"
        );
        assert_eq!(
            disconnect_banner("me@devbox", Some(12)),
            "\r\nDisconnected from me@devbox \u{2014} reconnecting in 12s, or press Enter\r\n"
        );
        // An early failure: never an automatic reconnect.
        assert_eq!(
            held_line("devbox", Held::Ended(127), None),
            "\r\nssh ended (exit 127) \u{2014} press Enter to reconnect\r\n"
        );
        assert_eq!(
            held_line("devbox", Held::Dropped, Some(3)),
            disconnect_banner("devbox", Some(3))
        );
    }

    #[test]
    fn each_new_session_gets_its_own_tmux_name() {
        let a = session_target(tmux("devbox", None), true);
        let b = session_target(tmux("devbox", Some("devbox-000001")), true);
        let (a, b) = (a.tmux_name.unwrap(), b.tmux_name.unwrap());
        assert_ne!(a, b, "two panes to one host don't share a tmux session");
        let (host, hex) = a.rsplit_once('-').unwrap();
        assert_eq!(host, "devbox");
        assert!(
            hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()),
            "{a}"
        );
        // tmux-safe, and at most 32 after `fuxx-`: a long host is cut.
        let long = session_target(tmux(&format!("me@{}", "x".repeat(40)), None), true);
        let long = long.tmux_name.unwrap();
        assert!(long.len() <= 32, "{long}");
        assert!(long.starts_with("me_xxx"), "{long}");
        // Without tmux there's none.
        let plain = SshTarget {
            tmux: false,
            ..tmux("devbox", Some("devbox-000001"))
        };
        assert_eq!(session_target(plain, true).tmux_name, None);
    }

    #[test]
    fn a_reconnect_or_restore_keeps_the_tmux_name() {
        let kept = session_target(tmux("devbox", Some("devbox-0a0b0c")), false);
        assert_eq!(kept.tmux_name.as_deref(), Some("devbox-0a0b0c"));
        // ssh attaches to that very tmux session.
        let argv = argv_for(&kept, &Effective::default(), None);
        assert_eq!(
            argv[argv.len() - 5..],
            ["tmux", "new", "-A", "-s", "fuxx-devbox-0a0b0c"]
        );
        // An entry saved before names existed gets one.
        assert!(
            session_target(tmux("devbox", None), false)
                .tmux_name
                .is_some()
        );
    }

    #[test]
    fn a_saved_session_with_a_bad_host_is_not_launched() {
        let mut ok = ssh_launch(" devbox ");
        assert_eq!(check_restored(&mut ok), Ok(()));
        let target = ok.saved.ssh.unwrap();
        assert_eq!(target.host, "devbox", "trimmed");
        assert!(target.tmux_name.is_some(), "named");
        assert!(check_restored(&mut ssh_launch("-oProxyCommand=x")).is_err());
        let (kept, refused) = checked_launches(vec![
            (0, Launch::default()),
            (1, ssh_launch("-oProxyCommand=x")),
            (2, ssh_launch("devbox")),
            (3, ssh_launch("dev\nbox")),
        ]);
        assert_eq!(kept.len(), 2);
        assert_eq!(
            kept[0],
            (0, Launch::default()),
            "a local session is untouched"
        );
        // Each keeps its place in the sessions file.
        assert_eq!(kept[1].0, 2);
        assert_eq!(kept[1].1.saved.ssh.as_ref().unwrap().host, "devbox");
        assert_eq!(refused.len(), 2);
    }

    #[test]
    fn only_the_first_drop_of_a_run_is_news() {
        // The connection drops: notified (per the M4 rules), whether or not it was up.
        assert!(drop_is_news(false, true));
        assert!(drop_is_news(false, false), "even one that never got far");
        // Each reconnect that drops again before it was up: the same drop, no notice.
        assert!(!drop_is_news(true, false));
        assert!(!drop_is_news(true, false), "a second failed try in a row");
        // A reconnect that was up, then dropped: a new drop.
        assert!(drop_is_news(true, true));
    }

    #[test]
    fn a_connection_is_up_once_it_printed_for_a_while() {
        // No output at all: a try stuck 75 s in TCP connect was never up.
        assert!(!was_up(None));
        // ssh's own error, printed as it gives up: not up either.
        assert!(!was_up(Some(Duration::ZERO)));
        assert!(!was_up(Some(RETRIES_FORGOTTEN - Duration::from_secs(1))));
        // Output for a minute: it was really connected.
        assert!(was_up(Some(RETRIES_FORGOTTEN)));
        assert!(was_up(Some(Duration::from_secs(3600))));
    }

    #[test]
    fn the_login_environment_is_parsed_from_env_0() {
        // An rc file that prints before env: its text is not part of the first name.
        let text = "Welcome!\nPATH=/opt/homebrew/bin:/usr/bin\0SSH_AUTH_SOCK=/a b/agent\0\
                    NOTE=two\nlines\0EMPTY=\0SHLVL=2\0PWD=/x\0OLDPWD=/y\0_=/usr/bin/env\0\
                    bad name=1\0=nameless\0no-equals\0trailing junk";
        assert_eq!(
            parse_env(text),
            [
                ("PATH", "/opt/homebrew/bin:/usr/bin"),
                ("SSH_AUTH_SOCK", "/a b/agent"),
                ("NOTE", "two\nlines"),
                ("EMPTY", ""),
            ]
            .map(|(k, v)| (k.to_string(), v.to_string()))
        );
        assert!(parse_env("").is_empty());
    }

    #[test]
    fn the_login_environment_comes_from_the_shell() {
        let dir = std::env::temp_dir().join(format!("fuxx-login-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let script = |name: &str, body: &str| {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        // Asked for exactly this: an interactive login shell running `command env -0`.
        let shell = script(
            "shell",
            "[ \"$1 $2\" = \"-ilc command env -0\" ] || exit 3\n\
             read line && exit 4\n\
             printf 'PATH=/opt/homebrew/bin:/usr/bin\\0SSH_AUTH_SOCK=/agent\\0'",
        );
        // Generous: a freshly written script can take seconds to start under a full run.
        let enough = Duration::from_secs(10);
        assert_eq!(
            capture_login_env(shell.as_os_str(), enough),
            Some(vec![
                ("PATH".to_string(), "/opt/homebrew/bin:/usr/bin".to_string()),
                ("SSH_AUTH_SOCK".to_string(), "/agent".to_string()),
            ])
        );
        // A shell that fails, prints nothing, or hangs: none (fuxx's own is used).
        let failing = script("failing", "printf 'A=1\\0'; exit 1");
        assert_eq!(capture_login_env(failing.as_os_str(), enough), None);
        let silent = script("silent", "exit 0");
        assert_eq!(capture_login_env(silent.as_os_str(), enough), None);
        let hung = script("hung", "sleep 30");
        let start = Instant::now();
        assert_eq!(
            capture_login_env(hung.as_os_str(), Duration::from_millis(300)),
            None
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn the_reconnect_line_resets_the_screen_first() {
        let (tx, _rx) = futures::channel::mpsc::unbounded();
        let mut cmd = portable_pty::CommandBuilder::new("/bin/sh");
        // TOP on the main screen; then a full-screen program: alternate screen, a two-line
        // scroll region, red text.
        cmd.args([
            "-c",
            "printf 'TOP\\n\\033[?1049h\\033[2;3r\\033[31malt'; sleep 5",
        ]);
        let core =
            TerminalCore::spawn(cmd, TermDims { cols: 60, rows: 5 }, tx, Theme::default()).unwrap();
        let screen = |core: &TerminalCore| core.text(crate::grid::TextScope::Screen);
        let start = Instant::now();
        while !screen(&core).contains("alt") && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(core.mode().contains(TermMode::ALT_SCREEN));
        core.show_local(&held_screen("devbox", Held::Dropped, None));
        assert!(
            !core.mode().contains(TermMode::ALT_SCREEN),
            "back on the main screen"
        );
        let text = screen(&core);
        assert!(text.contains("Disconnected from devbox"), "{text}");
        assert!(!text.contains("alt"), "{text}");
        // In the default colour.
        let snap = core.snapshot();
        let row = snap
            .lines
            .iter()
            .find(|l| {
                l.iter()
                    .map(|c| c.ch)
                    .collect::<String>()
                    .contains("Disconnected")
            })
            .unwrap();
        assert_eq!(row[0].fg, Theme::default().foreground);
        // The whole screen scrolls again: enough lines push TOP off.
        core.show_local("1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
        assert!(!screen(&core).contains("TOP"), "{}", screen(&core));
    }
}
