//! What the shell prompt's command does to a session (M11q): `Session::command` follows the
//! marks the view turned into `Change`s.

use gpui::Context;

use crate::command_name::Change;
use crate::workspace::Workspace;

impl Workspace {
    /// Session `index`'s command changed. A held (disconnected) session keeps no name: a late
    /// mark changes nothing. Always redraws: the chrome parts observe the workspace.
    pub(crate) fn command_changed(
        &mut self,
        index: usize,
        change: &Change,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self
            .sessions
            .get_mut(index)
            .filter(|s| s.disconnected.is_none())
        {
            session.command = match change {
                Change::Started(short) => short.clone().map(Into::into),
                Change::Ended(_) | Change::Cleared => None,
            };
        }
        cx.notify();
    }
}
