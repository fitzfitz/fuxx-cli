//! Taking back a message the agent hasn't read yet (batch 3, wave 3's UX gap): × on its
//! bubble. An agent that can (`Provider::cancels_queued`: Claude's `cancel_async_message`) is
//! asked to drop it from its queue; a message fuxx still holds (`queued`, before the process is
//! there) is dropped before it is ever written, for any agent.

use super::*;

/// Message `item`'s id when × can ask the agent to take it back: yours, named, not read yet,
/// with an agent that `cancels`.
pub(crate) fn cancel_id(item: &Item, cancels: bool) -> Option<&str> {
    match item {
        Item::You {
            id: Some(id),
            taken: false,
            ..
        } if cancels => Some(id),
        _ => None,
    }
}

/// × on a held message's bubble, and the words for both.
pub(crate) const CANCEL_TIP: &str = "Cancel this message (not read yet)";
/// The note when the agent had taken the message in already.
pub(crate) const CANCEL_LATE: &str = "Already read: it can't be cancelled";

impl AgentTab {
    /// Whether × shows on message `item`: the agent can take it back, its process runs, and it
    /// hasn't been asked to already.
    pub(super) fn cancel_offered(&self, item: &Item) -> bool {
        let cancels = self.provider.is_some_and(|p| p.cancels_queued());
        cancel_id(item, cancels).is_some_and(|id| !self.cancelling.contains(id)) && self.running()
    }

    /// × on message `ix`: the agent is asked to drop it (its answer, `queued_cancelled`, takes
    /// it out of the chat).
    pub(crate) fn cancel_message(&mut self, ix: usize, cx: &mut Context<Self>) {
        let cancels = self.provider.is_some_and(|p| p.cancels_queued());
        let Some(id) = self
            .conversation
            .items()
            .get(ix)
            .and_then(|i| cancel_id(i, cancels))
            .map(str::to_string)
        else {
            return;
        };
        if self.cancelling.insert(id.clone()) {
            self.write(&Action::CancelQueued { id });
            cx.notify();
        }
    }

    /// The agent's answer to a cancel: gone, it leaves the chat (and its turn is never waited
    /// on); taken in already, a note says so.
    pub(super) fn queued_cancelled(&mut self, id: &str, cancelled: bool, cx: &mut Context<Self>) {
        if !self.cancelling.remove(id) {
            return;
        }
        if cancelled {
            self.conversation.cancel_unread(id);
        } else {
            self.notice(CANCEL_LATE.to_string(), cx);
        }
        self.after_change(true, cx);
    }

    /// × on a message fuxx still holds (`queued`, waiting for the process): it goes, never
    /// written.
    pub(crate) fn cancel_held(&mut self, i: usize, cx: &mut Context<Self>) {
        if i < self.queued.len() {
            self.queued.remove(i);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn you(id: Option<&str>, taken: bool) -> Item {
        Item::You {
            text: "note".into(),
            attachments: Vec::new(),
            id: id.map(str::to_string),
            taken,
        }
    }

    #[test]
    fn only_an_unread_named_message_can_be_taken_back_and_only_by_an_agent_that_can() {
        assert_eq!(cancel_id(&you(Some("u1"), false), true), Some("u1"));
        assert_eq!(cancel_id(&you(Some("u1"), true), true), None, "read");
        assert_eq!(cancel_id(&you(None, false), true), None, "no id");
        assert_eq!(cancel_id(&you(Some("u1"), false), false), None, "agy");
        assert_eq!(cancel_id(&Item::Notice("x".into()), true), None);
    }
}
