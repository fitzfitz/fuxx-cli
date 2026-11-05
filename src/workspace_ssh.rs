//! M10b: SSH sessions in the workspace. Opening one (the host checked, then its ssh settings
//! read off the UI thread, and `ssh` run as the pane's program), holding the pane when the
//! connection fails (ssh exits 255) with a reconnect line, and reconnecting it in place (Enter,
//! or with tmux a timer). Grouping by host is in `groups`, saving in `restore`.

use std::path::Path;
use std::time::{Duration, Instant};

use gpui::{Context, Entity, EntityId, Task, Window, prelude::*};

use crate::api_events::Event;
use crate::restore::{Launch, SavedSession};
use crate::split_tree::Axis;
use crate::ssh_target::{self, Effective, SshTarget};
use crate::status::Status;
use crate::terminal_view::TerminalView;
use crate::workspace::{At, Opened, Session, Workspace, home_dir};
use crate::workspace_daemon::open_refusal;
use crate::{ssh_control, ssh_hosts};

/// At most this long each for `ssh -G` and the shared-connection check before an SSH session
/// opens.
const SSH_CHECK: Duration = Duration::from_secs(2);

/// An SSH connection that lasted this long starts the automatic reconnect waits over.
const RETRIES_FORGOTTEN: Duration = Duration::from_secs(60);

/// A held SSH session (M10b): its screen stays, with the reconnect line.
pub(crate) struct Disconnected {
    /// With tmux, the automatic reconnect; dropped (the pane closed, or it reconnected),
    /// it never fires.
    _timer: Option<Task<()>>,
    /// A reconnect is under way (so Enter and the timer don't both start one).
    reconnecting: bool,
}

/// Where an SSH session opens: a layout of its own, a pane beside (Row) or under (Column)
/// session `of`, or in place of session `index` (a reconnect: same pane, name and group).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    New,
    Split { of: usize, axis: Axis },
    Replace(usize),
}

/// What a session's program ending does to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitAction {
    /// It is taken away, as always.
    Close,
    /// An SSH session whose connection failed: the pane stays for a reconnect.
    Hold,
}

/// `Hold` only for an SSH session whose ssh exited 255 ("connection failed"); a logout, the
/// remote shell's own code, or an unknown one closes it as any session.
pub fn on_exit(ssh: Option<&SshTarget>, code: Option<u32>) -> ExitAction {
    if ssh.is_some() && code == Some(255) {
        ExitAction::Hold
    } else {
        ExitAction::Close
    }
}

/// The wait before automatic reconnect `attempt` (0-based, tmux sessions): 3 s, doubling
/// each time, at most 60 s.
pub fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs((3u64 << attempt.min(5)).min(60))
}

/// The line a held SSH session shows, on a line of its own; `next`: the seconds until the
/// automatic reconnect (tmux). `host` passed `validate_host`, so it holds no control text.
pub fn disconnect_banner(host: &str, next: Option<u64>) -> String {
    let line = match next {
        Some(secs) => format!("Disconnected from {host} — reconnecting in {secs}s, or press Enter"),
        None => format!("Disconnected from {host} — press Enter to reconnect"),
    };
    format!("\r\n{line}\r\n")
}

/// The argv an SSH session runs: connection reuse and keepalive only where the host's
/// `ssh -G` shows them unset, and the shared connection checked first when fuxx's own is
/// used. With `recent`, the host is recorded in that recent-hosts file. Blocks (ssh runs),
/// so off the UI thread.
pub(crate) fn ssh_program(target: &SshTarget, recent: Option<&Path>) -> Vec<String> {
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
    ssh_target::ssh_argv(target, &target.host, &effective, control.as_deref())
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
    /// A reconnected SSH session takes the place of the held one (by view `old`): its row, its
    /// pane in the layout, its name, note and progress. Scripts see the old one close and the
    /// new one open in the same row. False if `old` is gone (its pane was closed meanwhile).
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
        self.emit(&Event::Closed, Some(index));
        self.check_waits(&[old_id], cx);
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
        let Some(session) = self.sessions.get_mut(index) else {
            return false;
        };
        // Dropping the held session's view (and with it its timer).
        *session = Session {
            id: session_id,
            view,
            status: Status::Idle,
            title: None,
            name: saved.name,
            project: saved.project,
            preset: saved.preset,
            note: session.note.take(),
            progress: session.progress.take(),
            lost: false,
            ssh: saved.ssh,
            disconnected: None,
            retries: session.retries,
            opened_at: Instant::now(),
            _subscription: subscription,
        };
        self.emit(&Event::Opened, Some(index));
        // The background service's new session hasn't had the note and progress.
        self.push_meta(index, cx);
        // The new view took the focus as it was made: back to the active pane.
        self.activate(window, cx);
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
        let target = match ssh_target::validate_host(&target.host) {
            Ok(host) => SshTarget { host, ..target },
            Err(why) => {
                self.ssh_failed(&why, cx);
                return not_opened(self, then, window, cx);
            }
        };
        let view_of = |i: usize| self.sessions.items().get(i).map(|s| s.view.entity_id());
        let (at, name) = match place {
            Place::New => (At::New, None),
            Place::Split { of, axis } => {
                (view_of(of).map_or(At::New, |v| At::Split(axis, v)), None)
            }
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

    /// An SSH session couldn't open (`why`): logged, and said at the foot of the sidebar.
    pub(crate) fn ssh_failed(&mut self, why: &str, cx: &mut Context<Self>) {
        eprintln!("[fuxx] ssh: session not started: {why}");
        self.notice = Some(format!("SSH session not started: {why}").into());
        cx.notify();
    }

    /// The active session's SSH target and index, if it is an SSH session.
    pub(crate) fn active_ssh(&self) -> Option<(usize, SshTarget)> {
        let index = self.sessions.active_index()?;
        let target = self.sessions.items()[index].ssh.clone()?;
        Some((index, target))
    }

    /// SSH session `index`'s connection failed (ssh exited 255): it stays, with its screen and
    /// the reconnect line, and its ring shows done. With tmux it reconnects by itself after a
    /// wait that grows with each try in a row (the remote programs are there to come back to).
    pub(crate) fn hold(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        let Some(target) = session.ssh.clone() else {
            return;
        };
        // A connection that held a while starts the waits over.
        if session.opened_at.elapsed() >= RETRIES_FORGOTTEN {
            session.retries = 0;
        }
        let wait = target.tmux.then(|| reconnect_delay(session.retries));
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
        session.disconnected = Some(Disconnected {
            _timer: timer,
            reconnecting: false,
        });
        eprintln!(
            "[fuxx] ssh: s{} disconnected from {}",
            session.id, target.host
        );
        let banner = disconnect_banner(&target.host, wait.map(|w| w.as_secs()));
        session.view.update(cx, |view, cx| view.hold(&banner, cx));
        self.set_status(index, Status::Done, cx);
        // Its background-service session has ended: one fewer keeps running.
        self.sync_quit_title(cx);
    }

    /// Reconnect held SSH session `index` in place (Enter, or the tmux timer when `auto`):
    /// same pane, name, group, host and option, on a fresh screen. If it can't start, the
    /// pane stays held for another Enter.
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
        session.retries = if auto {
            session.retries.saturating_add(1)
        } else {
            0
        };
        let view = session.view.entity_id();
        let failed: Opened = Box::new(move |this, id, _, _| {
            if id.is_none()
                && let Some(i) = this.sessions.position(|s| s.view.entity_id() == view)
                && let Some(held) = this
                    .sessions
                    .get_mut(i)
                    .and_then(|s| s.disconnected.as_mut())
            {
                held.reconnecting = false;
            }
        });
        self.open_ssh_then(target, Place::Replace(index), Some(failed), window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{ExitAction, disconnect_banner, on_exit, reconnect_delay};
    use crate::ssh_target::SshTarget;
    use std::time::Duration;

    #[test]
    fn on_exit_holds_only_ssh_255() {
        let ssh = SshTarget {
            host: "devbox".into(),
            tmux: false,
        };
        // ssh's "connection failed" code: the pane stays.
        assert_eq!(on_exit(Some(&ssh), Some(255)), ExitAction::Hold);
        // A normal logout, the remote shell's own code, or one not known: closed as today.
        for code in [Some(0), Some(1), Some(130), None] {
            assert_eq!(on_exit(Some(&ssh), code), ExitAction::Close, "{code:?}");
        }
        // A local session that exits 255 closes as always.
        assert_eq!(on_exit(None, Some(255)), ExitAction::Close);
        assert_eq!(on_exit(None, Some(0)), ExitAction::Close);
    }

    #[test]
    fn reconnect_delays() {
        let secs: Vec<u64> = (0..8).map(|n| reconnect_delay(n).as_secs()).collect();
        assert_eq!(secs, [3, 6, 12, 24, 48, 60, 60, 60]);
        assert_eq!(reconnect_delay(u32::MAX), Duration::from_secs(60));
    }

    #[test]
    fn banner_texts() {
        assert_eq!(
            disconnect_banner("devbox", None),
            "\r\nDisconnected from devbox \u{2014} press Enter to reconnect\r\n"
        );
        assert_eq!(
            disconnect_banner("me@devbox", Some(12)),
            "\r\nDisconnected from me@devbox \u{2014} reconnecting in 12s, or press Enter\r\n"
        );
    }
}
