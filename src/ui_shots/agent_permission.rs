//! Chore 25's scenes (note 93: "redesign this"): the open permission card with a short command,
//! a long heredoc (folded, then opened), a tool with no readable form (its input behind "Show
//! input"), and a narrow pane.

use gpui::{Entity, VisualTestAppContext, px};

use super::agent::feed;
use super::agent_activity::{DIMS, chevron_ends, click, give, part, shoot};
use super::{Kind, Scene, Themes, frame_fresh};
use crate::agent_tab::AgentTab;
use crate::providers::Event;
use crate::status::Status;

pub(super) const FILE: &str = file!();

const SOURCES: &[&str] = &[
    "src/agent_permission_card.rs",
    "src/agent_card_parts.rs",
    "src/agent_rows.rs",
    "src/agent_tab.rs",
    "src/agent_conversation.rs",
];

const fn scene(name: &'static str, run: fn()) -> Scene {
    Scene {
        name,
        run,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    }
}

pub(super) const SCENES: &[Scene] = &[
    scene("agent-permission-short", agent_permission_short),
    scene("agent-permission-heredoc", agent_permission_heredoc),
    scene("agent-permission-input", agent_permission_input),
    scene("agent-permission-narrow", agent_permission_narrow),
];

/// A commit with its message in a heredoc, and the log line after it (13 lines).
const HEREDOC: &str = "git commit -F - <<'MSG'
Retry against the current endpoint after a failover

The retry loop captured the endpoint once, before the first attempt,
so every retry after a failover kept hitting the old host.

- read the endpoint from the client on each attempt
- drop the cached base_url from the retry state
- add a test that fails over between two attempts

Refs: retry.rs, client.rs
MSG
git log --oneline -1";

/// Claude asks to run Bash `command` (`about`: its description; `always`: the rule offered).
fn ask_bash(
    cx: &mut VisualTestAppContext,
    tab: &Entity<AgentTab>,
    command: &str,
    about: &str,
    always: &str,
) {
    feed(cx, tab, |t, cx| t.send("Commit the retry fix".into(), cx));
    give(
        cx,
        tab,
        vec![Event::Permission {
            id: "p1".into(),
            tool: "Bash".into(),
            what: command.into(),
            detail: String::new(),
            always: Some(always.into()),
            about: Some(about.into()),
            by: None,
            plan: None,
        }],
    );
    assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Waiting);
}

/// The answers in one row of slim buttons with the quiet hint on it, inside the card.
fn one_row(n: &str, cx: &VisualTestAppContext, tab: &Entity<AgentTab>) {
    let buttons = part(cx, tab, n, "permission-buttons");
    let allow = part(cx, tab, n, "permission-allow");
    let hint = part(cx, tab, n, "permission-hint");
    let card = part(cx, tab, n, "permission-card");
    assert!(buttons.size.height <= px(25.), "{n}: one row ({buttons:?})");
    assert!(
        (allow.size.height + px(2.) - px(24.)).abs() < px(0.5),
        "{n}: 24 pt buttons ({allow:?})"
    );
    assert!(
        (hint.center().y - allow.center().y).abs() < px(1.5),
        "{n}: the hint on the buttons' row"
    );
    assert!(buttons.right() <= card.right(), "{n}: inside the card");
}

/// A one-line command: its description as the title, `Bash` as a tag, the command once, no
/// fold and no raw input.
pub(super) fn agent_permission_short() {
    shoot("agent-permission-short", DIMS, |n, cx, window, tab, _| {
        ask_bash(
            cx,
            tab,
            "cargo test retry",
            "Run the retry tests",
            "Always allow cargo test *",
        );
        let _ = frame_fresh(cx, window);
        let title = part(cx, tab, n, "permission-title");
        let tag = part(cx, tab, n, "permission-tag");
        assert!(tag.left() >= title.right(), "{n}: the tag after the title");
        let _ = part(cx, tab, n, "permission-command");
        let drawn = |name: &str| cx.read(|cx| tab.read(cx).shot_part(name)).is_some();
        assert!(
            !drawn("permission-more") && !drawn("permission-input"),
            "{n}: nothing folded"
        );
        one_row(n, cx, tab);
    });
}

/// A long heredoc folds after six lines ("… 7 more lines · Show all" and a chevron); Show all
/// opens it whole.
pub(super) fn agent_permission_heredoc() {
    shoot("agent-permission-heredoc", DIMS, |n, cx, window, tab, _| {
        ask_bash(
            cx,
            tab,
            HEREDOC,
            "Create the commit and show it",
            "Always allow git commit *",
        );
        let _ = frame_fresh(cx, window);
        let folded = part(cx, tab, n, "permission-command");
        chevron_ends(n, cx, tab, "permission-more");
        one_row(n, cx, tab);
        click(cx, window, part(cx, tab, n, "permission-more"));
        let _ = frame_fresh(cx, window);
        let open = part(cx, tab, n, "permission-command");
        assert!(
            open.size.height > folded.size.height * 1.8,
            "{n}: opened whole ({folded:?} → {open:?})"
        );
        chevron_ends(n, cx, tab, "permission-more");
        // Back to its fold for the shot.
        click(cx, window, part(cx, tab, n, "permission-more"));
        let _ = frame_fresh(cx, window);
        let again = part(cx, tab, n, "permission-command");
        assert_eq!(again.size.height, folded.size.height, "{n}: folded again");
    });
}

/// A tool with no readable form beyond its file (Write): no tag, "Claude wants to run Write",
/// its input behind "Show input".
pub(super) fn agent_permission_input() {
    shoot("agent-permission-input", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| t.send("Create notes.md".into(), cx));
        give(
            cx,
            tab,
            vec![Event::Permission {
                id: "p1".into(),
                tool: "Write".into(),
                what: "notes.md".into(),
                detail:
                    "{\n  \"file_path\": \"/w/my-app/notes.md\",\n  \"content\": \"# Notes\"\n}"
                        .into(),
                always: Some("Allow all edits this session".into()),
                about: None,
                by: None,
                plan: None,
            }],
        );
        let _ = frame_fresh(cx, window);
        let drawn = |cx: &VisualTestAppContext, name: &str| {
            cx.read(|cx| tab.read(cx).shot_part(name)).is_some()
        };
        assert!(
            !drawn(cx, "permission-tag"),
            "{n}: no tag (the title names the tool)"
        );
        assert!(!drawn(cx, "permission-input-text"), "{n}: the input folded");
        chevron_ends(n, cx, tab, "permission-input");
        click(cx, window, part(cx, tab, n, "permission-input"));
        let _ = frame_fresh(cx, window);
        assert!(
            drawn(cx, "permission-input-text"),
            "{n}: Show input opens it"
        );
        one_row(n, cx, tab);
    });
}

/// A narrow pane: the title cut inside the card, the answers wrapping inside it.
pub(super) fn agent_permission_narrow() {
    shoot(
        "agent-permission-narrow",
        (420.0, 640.0),
        |n, cx, window, tab, _| {
            ask_bash(
                cx,
                tab,
                HEREDOC,
                "Create the commit with the full message and show the log line after it",
                "Always allow git commit *",
            );
            let _ = frame_fresh(cx, window);
            let card = part(cx, tab, n, "permission-card");
            for name in [
                "permission-title",
                "permission-tag",
                "permission-buttons",
                "permission-command",
            ] {
                let p = part(cx, tab, n, name);
                assert!(
                    p.right() <= card.right() + px(0.5) && p.left() >= card.left(),
                    "{n}: {name} inside the card ({p:?} vs {card:?})"
                );
            }
        },
    );
}
