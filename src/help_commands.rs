// The Command Reference list: what `fuxx help` says, parsed into entries ready to show.
// Pure: it reads text and builds text. The panel that shows it is elsewhere.

use std::path::Path;

/// One command (or one form of it) in the reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandEntry {
    /// The section it sits under.
    pub group: &'static str,
    /// The usage line(s), each trimmed.
    pub usage: Vec<String>,
    /// What it does, on one line.
    pub description: String,
}

/// The whole reference: the entries, then the closing notes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub entries: Vec<CommandEntry>,
    pub footer: Vec<String>,
}

/// The column where descriptions start in `fuxx help`'s text.
const DESC_COL: usize = 39;

/// The section a command's first word belongs to. An unknown word lands in "fuxx itself".
fn group_of(word: &str) -> &'static str {
    match word {
        "ls" | "new" | "close" | "focus" | "rename" => "Sessions",
        "send" | "key" | "paste" | "read" | "status" | "question" | "answer" | "wait"
        | "events" => "Driving a session",
        "note" | "progress" | "notify" => "Sidebar and banners",
        "equalize" | "presets" => "Layout",
        "browser" => "Browser",
        _ => "fuxx itself",
    }
}

/// Split a line into its usage part and description part, if it has both: at `DESC_COL` when
/// the text lines up there, else at a run of three spaces (a usage that overflows the column).
fn split_line(line: &str) -> Option<(&str, &str)> {
    let b = line.as_bytes();
    if b.len() > DESC_COL
        && b[DESC_COL - 1] == b' '
        && b[DESC_COL] != b' '
        && line.is_char_boundary(DESC_COL)
    {
        return Some((line[..DESC_COL].trim(), &line[DESC_COL..]));
    }
    let text = line.trim_start();
    let at = text.find("   ")?;
    Some((&text[..at], text[at..].trim_start()))
}

/// An entry being built: its usage lines and the pieces of its description.
struct Pending {
    usage: Vec<String>,
    desc: Vec<String>,
}

impl Pending {
    fn finish(self) -> Option<CommandEntry> {
        let first = self.usage.first()?;
        let group = group_of(first.split_whitespace().next().unwrap_or(""));
        Some(CommandEntry {
            group,
            usage: self.usage,
            description: self.desc.join(" "),
        })
    }
}

/// Parse `fuxx help`'s text.
pub fn parse_usage(usage: &str) -> Reference {
    let mut entries = Vec::new();
    let mut footer = Vec::new();
    let mut cur: Option<Pending> = None;
    for line in usage.lines() {
        if line.trim().is_empty() || line.starts_with("usage:") {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            footer.push(line.trim_end().to_string());
            continue;
        }
        if indent == 2 {
            if let Some(entry) = cur.take().and_then(Pending::finish) {
                entries.push(entry);
            }
            cur = Some(Pending {
                usage: Vec::new(),
                desc: Vec::new(),
            });
        }
        let split = split_line(line);
        if split.is_none() && indent == 7 && line.trim_start().starts_with("Every ") {
            footer.push(line.trim().to_string());
            continue;
        }
        let Some(p) = cur.as_mut() else { continue };
        match split {
            Some((u, d)) => {
                if !u.is_empty() {
                    p.usage.push(u.to_string());
                }
                p.desc.push(d.trim().to_string());
            }
            None if indent <= 6 => p.usage.push(line.trim().to_string()),
            None => p.desc.push(line.trim().to_string()),
        }
    }
    if let Some(entry) = cur.take().and_then(Pending::finish) {
        entries.push(entry);
    }
    Reference { entries, footer }
}

/// `path` as one shell word: single-quoted when it has whitespace or shell metacharacters.
fn shell_word(path: &str) -> String {
    let plain = !path.is_empty()
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+=:,@%~".contains(c));
    if plain {
        path.to_string()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

/// The entry for `claude mcp add`, which `fuxx help` does not list.
pub fn mcp_entry(exe: &Path) -> CommandEntry {
    CommandEntry {
        group: "MCP setup",
        usage: vec![format!(
            "claude mcp add fuxx -- {}",
            shell_word(&exe.display().to_string())
        )],
        description: "Lets Claude Code drive fuxx (scripting needs fuxx running).".to_string(),
    }
}

/// The reference for `exe`: `fuxx help`'s commands, then the MCP setup.
pub fn reference(exe: &Path) -> Reference {
    let mut r = parse_usage(crate::cli::USAGE);
    r.entries.push(mcp_entry(exe));
    r
}

/// Whether `entry` matches a search `query` (usage, description or group, ignoring case).
pub fn entry_matches(entry: &CommandEntry, query: &str) -> bool {
    let q = query.to_lowercase();
    entry.usage.iter().any(|u| u.to_lowercase().contains(&q))
        || entry.description.to_lowercase().contains(&q)
        || entry.group.to_lowercase().contains(&q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli;

    fn parsed() -> Reference {
        parse_usage(cli::USAGE)
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn thirty_five_entries_and_the_first_is_ls() {
        let r = parsed();
        assert_eq!(r.entries.len(), 35);
        assert_eq!(r.entries[0].usage, strs(&["ls"]));
        assert_eq!(
            r.entries[0].description,
            "list sessions (#N* focused, #N+ on screen)"
        );
    }

    #[test]
    fn new_has_two_usage_lines_and_a_joined_description() {
        let r = parsed();
        let e = r
            .entries
            .iter()
            .find(|e| e.usage[0].starts_with("new "))
            .unwrap();
        assert_eq!(
            e.usage,
            strs(&[
                "new [--folder DIR] [--preset NAME] [--run CMD] [--name NAME]",
                "[--split right|down | --tab [--of TARGET]] [--background]"
            ])
        );
        assert_eq!(
            e.description,
            "open a session in a workspace of its own (--split: a pane beside TARGET's, --tab: a tab in it); prints its id"
        );
    }

    #[test]
    fn progress_splits_at_its_overflowing_description() {
        let r = parsed();
        let e = r
            .entries
            .iter()
            .find(|e| e.usage[0].starts_with("progress"))
            .unwrap();
        assert_eq!(
            e.usage,
            strs(&["progress [TARGET] 0-100 [--label TEXT] | --clear"])
        );
        assert_eq!(e.description, "a progress bar under the row");
    }

    #[test]
    fn wait_has_two_usage_lines_and_its_notes() {
        let r = parsed();
        let e = r
            .entries
            .iter()
            .find(|e| e.usage[0].starts_with("wait "))
            .unwrap();
        assert_eq!(e.usage.len(), 2);
        assert!(
            e.description
                .starts_with("block until the condition holds (at once if it already does;"),
            "{}",
            e.description
        );
    }

    #[test]
    fn footer_is_the_closing_lines() {
        assert_eq!(
            parsed().footer,
            strs(&[
                "Every browser command takes --session TARGET (default: your session).",
                "Every command but daemon takes --json. TARGET is s4, #2 or a session name.",
                "Exit codes: 0 ok, 1 refused or nothing to show, 2 bad arguments, 3 fuxx not reachable, 124 wait timed out."
            ])
        );
    }

    #[test]
    fn every_command_has_an_entry() {
        let r = parsed();
        for (name, _) in cli::COMMANDS {
            let mut words = name.split_whitespace();
            let first = words.next().unwrap();
            let rest: Vec<&str> = words.collect();
            let found = r.entries.iter().any(|e| {
                let mut u = e.usage[0].split_whitespace();
                u.next() == Some(first) && {
                    let all: Vec<&str> =
                        e.usage.iter().flat_map(|l| l.split_whitespace()).collect();
                    rest.iter().all(|w| all.contains(w))
                }
            });
            assert!(found, "no entry for {name}");
        }
    }

    #[test]
    fn groups_come_in_order() {
        let r = reference(Path::new("/A/fuxx"));
        let mut seen: Vec<&str> = Vec::new();
        for e in &r.entries {
            if seen.last() != Some(&e.group) {
                seen.push(e.group);
            }
        }
        assert_eq!(
            seen,
            [
                "Sessions",
                "Driving a session",
                "Sidebar and banners",
                "Layout",
                "Browser",
                "fuxx itself",
                "MCP setup"
            ]
        );
        assert_eq!(r.entries.last().unwrap().group, "MCP setup");
    }

    #[test]
    fn groups_follow_the_first_word() {
        let r = parsed();
        let group_of = |w: &str| {
            r.entries
                .iter()
                .find(|e| e.usage[0].split_whitespace().next() == Some(w))
                .unwrap()
                .group
        };
        assert_eq!(group_of("close"), "Sessions");
        assert_eq!(group_of("events"), "Driving a session");
        assert_eq!(group_of("notify"), "Sidebar and banners");
        assert_eq!(group_of("presets"), "Layout");
        assert_eq!(group_of("daemon"), "fuxx itself");
    }

    #[test]
    fn mcp_entry_names_the_exe() {
        let e = mcp_entry(Path::new("/A/fuxx"));
        assert_eq!(e.group, "MCP setup");
        assert_eq!(e.usage, strs(&["claude mcp add fuxx -- /A/fuxx"]));
        assert_eq!(
            e.description,
            "Lets Claude Code drive fuxx (scripting needs fuxx running)."
        );
    }

    #[test]
    fn mcp_entry_quotes_a_path_with_spaces() {
        let e = mcp_entry(Path::new("/My Apps/fuxx"));
        assert_eq!(e.usage, strs(&["claude mcp add fuxx -- '/My Apps/fuxx'"]));
        let q = mcp_entry(Path::new("/It's/fuxx"));
        assert_eq!(q.usage, strs(&["claude mcp add fuxx -- '/It'\\''s/fuxx'"]));
    }

    #[test]
    fn matching_ignores_case_and_looks_at_group() {
        let r = parsed();
        let browser: Vec<_> = r.entries.iter().filter(|e| e.group == "Browser").collect();
        assert_eq!(browser.len(), 8);
        assert!(browser.iter().all(|e| entry_matches(e, "browser")));
        let snap: Vec<_> = r
            .entries
            .iter()
            .filter(|e| entry_matches(e, "SNAPSHOT"))
            .collect();
        assert!(
            snap.iter()
                .any(|e| e.usage[0].starts_with("browser snapshot"))
        );
        assert!(!r.entries.iter().any(|e| entry_matches(e, "zzz")));
        assert!(entry_matches(&r.entries[0], "sessions"));
    }

    #[test]
    fn an_unknown_command_does_not_panic() {
        let r = parse_usage("usage: x\n\n  frobnicate [A]      does things\n");
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].group, "fuxx itself");
    }
}
