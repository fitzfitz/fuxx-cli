//! What the shell prompt's command does to a session (M11q): `Session::command` follows the
//! marks the view turned into `Change`s, and a command's end mark settles the session's status
//! and says which command ended and how, once.

use gpui::Context;

use crate::command_name::{Change, Finished};
use crate::notifier;
use crate::status::Status;
use crate::workspace::Workspace;

impl Workspace {
    /// Session `index`'s command changed. An end mark is told (`end_command`). A held
    /// (disconnected) session keeps no name and its status: a late mark changes nothing.
    /// Always redraws: the chrome parts observe the workspace.
    pub(crate) fn command_changed(
        &mut self,
        index: usize,
        change: &Change,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self
            .sessions
            .get_mut(index)
            .filter(|s| s.disconnected.is_none())
        else {
            cx.notify();
            return;
        };
        session.command = match change {
            Change::Started(short) => short.clone().map(Into::into),
            Change::Ended(_) | Change::Cleared => None,
        };
        if let Change::Ended(finished) = change {
            self.end_command(index, finished, cx);
        }
        cx.notify();
    }

    /// The command at session `index`'s prompt ended (its end mark), for either backend: what
    /// `notifier::end_tells` says, so that each ending has at most one banner.
    /// - It settles the status on Done at once (`status::settles_on_end`, the rule its tracker
    ///   goes by, so that one's own Done, which the view sends after the command's event,
    ///   arrives as no change), and that change is told in the ending's words.
    /// - Already Done (the silence guess was first; a background service's tracker, the only
    ///   one for its sessions, may also say Done before the mark is heard): the ending goes to
    ///   the 🔔 list and to scripts once, with a banner only if this Done had none.
    pub(crate) fn end_command(&mut self, index: usize, fin: &Finished, cx: &mut Context<Self>) {
        let looking = self.sessions.active_index() == Some(index) && self.window_active;
        let Some(session) = self.sessions.items().get(index) else {
            return;
        };
        let tells = notifier::end_tells(
            session.status,
            fin.ran,
            looking,
            session.done_bannered,
            session.end_told,
        );
        if tells.settle {
            // Whether that change is news (the list, the banner) is `set_status_told`'s to
            // say, by the same rules `end_tells` goes by.
            self.set_status_told(index, Status::Done, true, Some(fin), cx);
        } else if tells.list
            && let Some(words) = notifier::words(Status::Done, None, None, None, None, Some(fin))
        {
            self.notify_terminal(index, Status::Done, &words, tells.banner, cx);
            if let Some(session) = self.sessions.get_mut(index) {
                session.end_told = true;
                session.done_bannered |= tells.banner;
            }
        }
    }
}
