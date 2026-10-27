//! Agent presets: the built-in three plus `agent-preset` lines from the settings file.
//! Pure and GPUI-free, so the rules are unit-tested.

use crate::settings::Entry;
use serde::{Deserialize, Serialize};

/// An agent the user can start in a new session: a menu name and the command line it runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub command: String,
}

/// The menu holds at most this many presets (one per digit key).
pub const MAX_PRESETS: usize = 9;

/// The presets that exist before any `agent-preset` line.
pub fn builtins() -> Vec<Preset> {
    ["claude", "opencode", "codex"]
        .into_iter()
        .map(|n| Preset {
            name: n.into(),
            command: n.into(),
        })
        .collect()
}

/// Built-ins, then each `agent-preset` entry in order: `name: command` adds or replaces,
/// `name:` removes, an empty value resets. Returns the presets and warnings for bad lines.
pub fn from_entries(entries: &[Entry]) -> (Vec<Preset>, Vec<String>) {
    let mut presets = builtins();
    let mut warnings = Vec::new();
    for entry in entries.iter().filter(|e| e.key == "agent-preset") {
        if entry.value.is_empty() {
            presets = builtins();
            continue;
        }
        let parsed = entry
            .value
            .split_once(':')
            .map(|(n, c)| (n.trim(), c.trim()))
            .filter(|(n, _)| !n.is_empty());
        let Some((name, command)) = parsed else {
            warnings.push(format!(
                "agent-preset \"{}\": expected \"name: command\"",
                entry.value
            ));
            continue;
        };
        if command.is_empty() {
            presets.retain(|p| p.name != name);
        } else if let Some(existing) = presets.iter_mut().find(|p| p.name == name) {
            existing.command = command.to_string();
        } else {
            presets.push(Preset {
                name: name.into(),
                command: command.into(),
            });
        }
    }
    if presets.len() > MAX_PRESETS {
        let skipped: Vec<String> = presets
            .split_off(MAX_PRESETS)
            .into_iter()
            .map(|p| p.name)
            .collect();
        warnings.push(format!(
            "agent-preset: only {MAX_PRESETS} presets fit, skipped {}",
            skipped.join(", ")
        ));
    }
    (presets, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(value: &str) -> Entry {
        Entry {
            key: "agent-preset".into(),
            value: value.into(),
        }
    }
    fn names(p: &[Preset]) -> Vec<&str> {
        p.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn builtins_are_claude_opencode_codex() {
        let b = builtins();
        assert_eq!(names(&b), ["claude", "opencode", "codex"]);
        assert!(b.iter().all(|p| p.command == p.name));
    }

    #[test]
    fn user_presets_add_replace_and_remove() {
        let (p, warnings) = from_entries(&[
            e("aider: aider --model sonnet"),
            e("claude: claude --continue \"fix: it\""),
            e("codex:"),
        ]);
        assert_eq!(names(&p), ["claude", "opencode", "aider"]);
        assert_eq!(p[0].command, "claude --continue \"fix: it\"");
        assert_eq!(p[2].command, "aider --model sonnet");
        assert!(warnings.is_empty());
    }

    #[test]
    fn an_empty_value_resets_to_the_builtins() {
        assert_eq!(from_entries(&[e("codex:"), e("")]).0, builtins());
    }

    #[test]
    fn bad_lines_and_overflow_are_skipped_with_warnings() {
        let mut entries = vec![e("nocolon"), e(": cmd")];
        for i in 0..7 {
            entries.push(e(&format!("p{i}: run{i}")));
        }
        let (p, warnings) = from_entries(&entries);
        assert_eq!(p.len(), 9);
        assert_eq!(p[8].name, "p5");
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn other_keys_are_ignored() {
        let other = Entry {
            key: "theme".into(),
            value: "x: y".into(),
        };
        assert_eq!(from_entries(&[other]).0, builtins());
    }
}
