//! Long notices and errors in the chat: each row as tall as its wrapped text, so the next
//! message never draws over its last line (the user's bug report, 2026-11-24).

use gpui::{AnyWindowHandle, Entity, VisualTestAppContext};

use super::agent::{feed, scene_sized};
use super::{Kind, Scene, Themes, frame_fresh};
use crate::agent_tab::AgentTab;
use crate::providers::Event;

pub(super) const FILE: &str = file!();

const SOURCES: &[&str] = &["src/agent_rows.rs", "src/agent_tab.rs", "src/agent_chat.rs"];

pub(super) const SCENES: &[Scene] = &[Scene {
    name: "agent-notice-long",
    run: agent_notice_long,
    themes: Themes::Both,
    serial: false,
    kind: Kind::State,
    sources: SOURCES,
}];

/// Claude Code's long safety-check denial, as its `permission_denied` notice brings it.
const DENIED: &str = "Permission for this command was denied by a built-in Claude Code safety check. \
The command deletes files under a path built from a shell variable, and the check cannot tell \
what that variable holds when the command runs.\n\n\
If $W is empty or unset, `rm -rf $W/build` becomes `rm -rf /build`, which is outside the project \
and may hold files that are not yours to delete.\n\n\
Quote the variable and make the shell stop when it is missing: write \"${W:?}\"/build instead, \
so an empty value is an error rather than a different path.\n\n\
You can also use a literal absolute path that you have checked exists inside the project, which \
removes the guesswork for both the check and anyone reading the command later.\n\n\
If you meant to delete something outside the project, ask the user first and explain which \
folder it is and why it has to go.\n\n\
The safe form is `rm -rf \"${W:?}\"/build`, which makes the shell stop with an error instead of \
running rm when $W is unset, so nothing outside the project can be removed by mistake.";

const NEXT: &str = "I'll quote the variable and run it again.";

/// A long error, as a failed turn can bring one.
const FAILED: &str = "The agent stopped: the API answered 529 Overloaded three times in a row \
while the turn was streaming, and the retries ran out before the reply was complete.\n\n\
Nothing it wrote after the last finished tool call was kept. The files it edited before then \
are as it left them, and the checks it ran last still stand.\n\n\
Send the message again to continue from here, or start a new conversation if the step it was \
on no longer matters; your earlier messages and the changed files stay either way.";

/// Two widths: the column fills the window at the narrow one and stops at `COLUMN_MAX` at the
/// wide one (where the row was measured shorter than drawn).
const WIDTHS: [f32; 2] = [720.0, 1100.0];

/// A long multi-paragraph notice, then a long error, each followed by the agent's message: at
/// two widths, each row's drawn text ends above the next row's top.
pub(super) fn agent_notice_long() {
    for width in WIDTHS {
        let name = format!("agent-notice-long-{width}");
        scene_sized(&name, (width, 900.0), check);
    }
}

fn check(n: &str, cx: &mut VisualTestAppContext, window: AnyWindowHandle, tab: &Entity<AgentTab>) {
    feed(cx, tab, |t, cx| t.send("Clean the build folder".into(), cx));
    // Rows: your message, the notice, the agent's reply.
    feed(cx, tab, |t, cx| {
        t.shot_feed(
            vec![Event::Notice(DENIED.into()), said("msg_next", NEXT)],
            cx,
        )
    });
    clears(n, "notice", 1, cx, window, tab);
    // Then an error and a reply under it.
    feed(cx, tab, |t, cx| {
        t.shot_feed(
            vec![Event::Error(FAILED.into()), said("msg_after", NEXT)],
            cx,
        )
    });
    clears(n, "error", 3, cx, window, tab);
}

fn said(id: &str, text: &str) -> Event {
    Event::MessageDone {
        message: id.into(),
        text: text.into(),
        parent: None,
    }
}

/// Part `part` (row `row`'s body, as drawn) ends at or above the next row's top, and its row
/// holds it.
fn clears(
    n: &str,
    part: &str,
    row: usize,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
) {
    let _ = frame_fresh(cx, window);
    let body = cx
        .read(|cx| tab.read(cx).shot_part(part))
        .unwrap_or_else(|| panic!("{n}: the {part} drawn"));
    let list = cx.read(|cx| tab.read(cx).shot_list());
    let own = list
        .bounds_for_item(row)
        .unwrap_or_else(|| panic!("{n}: row {row} (the {part}) laid out"));
    let next = list
        .bounds_for_item(row + 1)
        .unwrap_or_else(|| panic!("{n}: the row after the {part} laid out"));
    assert!(
        body.bottom() <= next.top() && body.bottom() <= own.bottom(),
        "{n}: the {part}'s text ends at {:?}, under the next row's top {:?} (its row ends at \
         {:?}): the row was measured shorter than drawn",
        body.bottom(),
        next.top(),
        own.bottom()
    );
}
