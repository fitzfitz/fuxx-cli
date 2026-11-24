//! A long conversation, made up (the agent tab's perf chore, 2026-11-24): turns of your message,
//! the agent's markdown, runs of tool calls and now and then a subagent's lane, about `items`
//! rows long. Deterministic. For the work-count tests, the bench and the long-chat scene.

use crate::providers::{Event, LaneControls, LaneEnd, Past};

/// The agent's words for turn `t`: a few paragraphs, a list and a code block, so the markdown
/// is real work to parse and hash.
fn said(t: usize) -> String {
    format!(
        "Turn {t}: I read the module and traced the call path.\n\n\
         - `rows_of` builds the order\n- `sync_rows` lays the rows out\n- the list draws them\n\n\
         ```rust\nfn step_{t}(x: usize) -> usize {{\n    x * 2 + {t}\n}}\n```\n\n\
         {}",
        "The change keeps each row's layout until its item changes. ".repeat(8)
    )
}

/// One tool call, started and finished (`parent`: a lane's step).
fn call(out: &mut Vec<Past>, id: String, parent: Option<String>) {
    out.push(Past::Event(Event::ToolStarted {
        id: id.clone(),
        name: "Bash".into(),
        summary: format!("cargo test {id}"),
        parent,
        detail: None,
        writes: Vec::new(),
        texts: Vec::new(),
    }));
    out.push(Past::Event(Event::ToolFinished {
        id,
        ok: true,
        summary: "ok".into(),
        output: Some("test result: ok. 12 passed".into()),
    }));
}

/// About `items` rows of history: each turn your message, the agent's narration, a run of one
/// to five calls, every seventh turn a subagent (its lane, three steps), and a closing message.
pub fn history(items: usize) -> Vec<Past> {
    let mut out = Vec::new();
    let mut rows = 0;
    let mut t = 0;
    while rows < items {
        out.push(Past::you(format!("Please look at part {t} of the code.")));
        out.push(Past::Event(Event::MessageDone {
            message: format!("m{t}a"),
            text: said(t),
            parent: None,
        }));
        rows += 2;
        for c in 0..(t % 5) + 1 {
            call(&mut out, format!("t{t}c{c}"), None);
            rows += 1;
        }
        if t % 7 == 3 {
            let lane = format!("lane{t}");
            call(&mut out, lane.clone(), None);
            out.push(Past::Event(Event::SubagentStarted {
                id: lane.clone(),
                parent: None,
                label: "Explore".into(),
                kind: "Explore".into(),
                task: format!("Find the callers of part {t}"),
                model: None,
                controls: LaneControls::default(),
            }));
            for s in 0..3 {
                call(&mut out, format!("{lane}s{s}"), Some(lane.clone()));
            }
            out.push(Past::Event(Event::SubagentEnded {
                id: lane,
                end: LaneEnd::Done,
            }));
            rows += 1;
        }
        out.push(Past::Event(Event::MessageDone {
            message: format!("m{t}b"),
            text: said(t + 1),
            parent: None,
        }));
        rows += 1;
        t += 1;
    }
    out
}

/// A conversation restored from `history(items)`.
#[cfg(test)]
pub fn conversation(items: usize) -> crate::agent_conversation::Conversation {
    let mut c = crate::agent_conversation::Conversation::new();
    c.past(history(items));
    c
}

/// One streamed piece of the agent's live message `message`.
pub fn delta(message: &str, text: &str) -> Event {
    Event::TextDelta {
        message: message.into(),
        text: text.into(),
        parent: None,
    }
}
