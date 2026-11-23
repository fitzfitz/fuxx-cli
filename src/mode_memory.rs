//! The modes an agent refused on this Mac (2026-11-24: an agy account with no Auto says
//! "--mode auto is not available … continuing in the default mode"): remembered, so a new tab
//! of that agent starts in the mode it ran in instead, with no warning. The mode asked stays
//! the provider's default; only this Mac's new tabs go round it.
//!
//! The file (`agent-modes.json`) sits beside the sessions file, `-dev` for a test build; unit
//! tests and scenes never read or write it (their memory lasts as long as their app).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gpui::{App, BorrowAppContext, Global};

/// Per agent (its provider's name): each mode it refused, and the mode it ran in instead.
pub type Refused = BTreeMap<String, BTreeMap<String, String>>;

/// The run's copy of what was refused (a GPUI global, read from the file the first time).
pub struct ModeMemory {
    refused: Refused,
}

impl Global for ModeMemory {}

impl Default for ModeMemory {
    fn default() -> Self {
        let refused = memory_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| read(&text))
            .unwrap_or_default();
        ModeMemory { refused }
    }
}

/// The file, under `$HOME`: the sessions file's folder, `-dev` for a test build.
pub fn path_for(home: &Path, dev: bool) -> PathBuf {
    let name = if dev {
        "agent-modes-dev.json"
    } else {
        "agent-modes.json"
    };
    home.join("Library/Application Support/fuxx").join(name)
}

/// This build's file; none in unit tests or scenes (they never touch yours) or without `$HOME`.
fn memory_path() -> Option<PathBuf> {
    if cfg!(any(test, feature = "ui-shots")) {
        return None;
    }
    crate::workspace::home_dir().map(|h| path_for(&h, cfg!(debug_assertions)))
}

/// The file's text as what was refused; anything unreadable is nothing remembered.
pub fn read(text: &str) -> Refused {
    serde_json::from_str(text).unwrap_or_default()
}

/// The mode a new tab of `agent` starts in: its `default`, unless this Mac remembers the agent
/// refusing it (then the mode it ran in instead).
pub fn start_mode(refused: &Refused, agent: &str, default: Option<&str>) -> Option<String> {
    let default = default?;
    Some(
        refused
            .get(agent)
            .and_then(|modes| modes.get(default))
            .cloned()
            .unwrap_or_else(|| default.to_string()),
    )
}

/// The quiet notice when an agent refuses a mode: "Auto isn't available on this agy account —
/// running in Accept edits" (the modes by their menu names, the agent by its short name).
pub fn refused_notice(asked: &str, who: &str, running: &str) -> String {
    format!("{asked} isn't available on this {who} account \u{2014} running in {running}")
}

/// The mode a new tab of `agent` starts in, as this run remembers (`start_mode`).
pub fn start_mode_in(cx: &App, agent: &str, default: Option<&str>) -> Option<String> {
    match cx.try_global::<ModeMemory>() {
        Some(m) => start_mode(&m.refused, agent, default),
        // Not read yet: the file says (read once, then kept).
        None => start_mode(&ModeMemory::default().refused, agent, default),
    }
}

/// `agent` refused mode `asked` and ran in `running`: remembered for the run, and written to
/// the file off the UI thread when it is news.
pub fn remember(cx: &mut App, agent: &str, asked: &str, running: &str) {
    let changed = cx.update_default_global(|m: &mut ModeMemory, _| {
        let modes = m.refused.entry(agent.to_string()).or_default();
        let news = modes.get(asked).map(String::as_str) != Some(running);
        modes.insert(asked.to_string(), running.to_string());
        news.then(|| m.refused.clone())
    });
    if let (Some(refused), Some(path)) = (changed, memory_path()) {
        std::thread::spawn(move || {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(text) = serde_json::to_string_pretty(&refused) {
                let _ = std::fs::write(path, text);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_default_starts_new_tabs_in_the_mode_it_ran_in() {
        let mut refused = Refused::new();
        assert_eq!(
            start_mode(&refused, "agy", Some("auto")).as_deref(),
            Some("auto"),
            "nothing refused: the default as asked"
        );
        refused
            .entry("agy".into())
            .or_default()
            .insert("auto".into(), "accept-edits".into());
        assert_eq!(
            start_mode(&refused, "agy", Some("auto")).as_deref(),
            Some("accept-edits")
        );
        assert_eq!(
            start_mode(&refused, "claude", Some("default")).as_deref(),
            Some("default"),
            "another agent's memory is its own"
        );
        assert_eq!(start_mode(&refused, "agy", None), None);
        // The file round-trips; anything unreadable remembers nothing.
        let text = serde_json::to_string(&refused).unwrap();
        assert_eq!(read(&text), refused);
        assert_eq!(read("not json"), Refused::new());
        assert_eq!(
            path_for(Path::new("/Users/me"), false),
            Path::new("/Users/me/Library/Application Support/fuxx/agent-modes.json")
        );
        assert!(memory_path().is_none(), "tests never touch yours");
    }

    #[test]
    fn the_refused_notice_names_both_modes() {
        assert_eq!(
            refused_notice("Auto", "agy", "Accept edits"),
            "Auto isn't available on this agy account \u{2014} running in Accept edits"
        );
    }
}
