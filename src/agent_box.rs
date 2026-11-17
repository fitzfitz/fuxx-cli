//! The agent box (M11ak): the chips under an agent tab's conversation that choose its model,
//! effort, mode and folder. This file holds the pure part: what a choice is (`Change`), when it
//! reaches the agent (`step_for`) and the line a restart leaves (`switched_notice`). The tab
//! (`agent_tab.rs`) carries it out.

use std::path::{Path, PathBuf};

use crate::providers::Applies;

/// A choice made in the box. An effort alone is a `Model` change with the current model's id.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    #[allow(dead_code)] // chosen by the model menu (a later M11ak task)
    Model {
        id: String,
        effort: Option<String>,
    },
    Mode(String),
    #[allow(dead_code)] // chosen by the folder chip (a later M11ak task)
    Folder(PathBuf),
}

/// When a change reaches the agent: now on its running process (`Live`), now by starting it
/// again on the same conversation (`RestartNow`), when the current turn ends (`AfterTurn`), or
/// only when it next starts (`NextStart`: the tab keeps it in its start flags).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Live,
    RestartNow,
    AfterTurn,
    NextStart,
}

/// The step for a change the provider takes as `applies`, the agent's process `running`, a
/// message `spoken` in the tab, a turn `working`.
pub fn step_for(applies: Applies, running: bool, spoken: bool, working: bool) -> Step {
    match (applies, spoken) {
        // Nothing said yet: live on a running agent, else it starts again with the change (no
        // process: only its start flags change).
        (Applies::Live, false) if running => Step::Live,
        (_, false) => Step::RestartNow,
        // This conversation never takes it.
        (Applies::NextConversation, true) => Step::NextStart,
        _ if working => Step::AfterTurn,
        (Applies::Live, true) if running => Step::Live,
        (Applies::Restart, true) if running => Step::RestartNow,
        _ => Step::NextStart,
    }
}

/// The quiet line a restart leaves in the conversation: `Switched to <model label> · <effort>`
/// (no effort part when none), `Switched to <mode label>`, `Moved to <~/path>`. A label not
/// given falls back to the id.
pub fn switched_notice(
    change: &Change,
    model_label: Option<&str>,
    mode_label: Option<&str>,
    home: Option<&Path>,
) -> String {
    match change {
        Change::Model { id, effort } => {
            let label = model_label.unwrap_or(id);
            match effort {
                Some(effort) => format!("Switched to {label} \u{b7} {effort}"),
                None => format!("Switched to {label}"),
            }
        }
        Change::Mode(id) => format!("Switched to {}", mode_label.unwrap_or(id)),
        Change::Folder(path) => {
            format!("Moved to {}", crate::sidebar::home_relative(path, home))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Applies::{Live, NextConversation, Restart};
    use Step::{AfterTurn, NextStart, RestartNow};

    #[test]
    fn step_for_covers_every_case() {
        // (applies, running, spoken, working) → step.
        let table = [
            // Before the first message: live on a running process, else a restart (which, with
            // no process, only changes how it starts).
            (Live, true, false, false, Step::Live),
            (Live, false, false, false, RestartNow),
            (Restart, true, false, false, RestartNow),
            (Restart, false, false, false, RestartNow),
            (NextConversation, true, false, false, RestartNow),
            (NextConversation, false, false, false, RestartNow),
            // While a turn runs: after it.
            (Live, true, true, true, AfterTurn),
            (Live, false, true, true, AfterTurn),
            (Restart, true, true, true, AfterTurn),
            (Restart, false, true, true, AfterTurn),
            // Idle after the first message.
            (Live, true, true, false, Step::Live),
            (Live, false, true, false, NextStart),
            (Restart, true, true, false, RestartNow),
            (Restart, false, true, false, NextStart),
            // Only a new conversation takes it: never this one.
            (NextConversation, true, true, false, NextStart),
            (NextConversation, false, true, false, NextStart),
            (NextConversation, true, true, true, NextStart),
            (NextConversation, false, true, true, NextStart),
        ];
        for (applies, running, spoken, working, want) in table {
            assert_eq!(
                step_for(applies, running, spoken, working),
                want,
                "{applies:?} running {running} spoken {spoken} working {working}"
            );
        }
    }

    #[test]
    fn switched_notices() {
        let model = |effort: Option<&str>| Change::Model {
            id: "sonnet".into(),
            effort: effort.map(String::from),
        };
        assert_eq!(
            switched_notice(
                &model(Some("medium")),
                Some("Claude Sonnet 5.5"),
                None,
                None
            ),
            "Switched to Claude Sonnet 5.5 \u{b7} medium"
        );
        assert_eq!(
            switched_notice(&model(None), Some("Claude Sonnet 5.5"), None, None),
            "Switched to Claude Sonnet 5.5"
        );
        assert_eq!(
            switched_notice(&model(None), None, None, None),
            "Switched to sonnet",
            "no label: the id"
        );
        assert_eq!(
            switched_notice(&Change::Mode("plan".into()), None, Some("Plan"), None),
            "Switched to Plan"
        );
        let home = Path::new("/home/me");
        assert_eq!(
            switched_notice(
                &Change::Folder(home.join("code/scratch")),
                None,
                None,
                Some(home)
            ),
            "Moved to ~/code/scratch"
        );
        assert_eq!(
            switched_notice(&Change::Folder("/tmp/x".into()), None, None, Some(home)),
            "Moved to /tmp/x",
            "outside home: as it is"
        );
    }
}
