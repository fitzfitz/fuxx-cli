//! What the shell prompt's command does to a session (M11q): `Session::command` follows the
//! marks the view turned into `Change`s, and a command's end mark settles the session's status.

use gpui::Context;

use crate::command_name::Change;
use crate::status::{self, Status};
use crate::workspace::Workspace;

impl Workspace {
    /// Session `index`'s command changed. An end mark settles its status on Done at once
    /// (`status::settles_on_end`, the rule its tracker goes by, so that one's own Done arrives
    /// as no change), for either backend. A held (disconnected) session keeps no name and its
    /// status: a late mark changes nothing. Always redraws: the chrome parts observe the
    /// workspace.
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
        let was = session.status;
        if let Change::Ended(finished) = change
            && status::settles_on_end(was, finished.ran)
        {
            self.set_status(index, Status::Done, true, cx);
        }
        cx.notify();
    }
}
