//! `--changed <base>`: which scenes a git change touches, and the line that says so.

use std::process::Command;

use super::{Affected, SHARED, Scene, affected};

/// The repo-relative files changed since `base` (`git diff <base>...HEAD`) or not yet committed
/// (staged, unstaged, untracked). Err names what git said when it fails or doesn't know `base`.
pub(super) fn changed_files(base: &str) -> Result<Vec<String>, String> {
    let range = format!("{base}...HEAD");
    let mut files: Vec<String> = Vec::new();
    for args in [
        vec!["diff", "--name-only", range.as_str()],
        vec!["diff", "--name-only", "HEAD"],
        vec!["ls-files", "--others", "--exclude-standard", "--full-name"],
    ] {
        let out = Command::new("git")
            .args(&args)
            .output()
            .map_err(|e| format!("git did not run: {e}"))?;
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr);
            return Err(format!(
                "git {} failed: {}",
                args.join(" "),
                why.trim().lines().next().unwrap_or("no message")
            ));
        }
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if !line.is_empty() && !files.iter().any(|f| f == line) {
                files.push(line.to_string());
            }
        }
    }
    Ok(files)
}

/// The scenes to run, from the `files` changed (or why they are unknown): `None` for all of them,
/// and the line that says what was chosen and why.
pub(super) fn decide(
    files: Result<Vec<String>, String>,
    scenes: &[Scene],
) -> (Option<Vec<&'static str>>, String) {
    let files = match files {
        Ok(files) => files,
        Err(why) => return (None, format!("all ({why})")),
    };
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    match affected(&refs, scenes, SHARED) {
        Affected::Nothing => (Some(vec![]), "nothing (no src changes)".into()),
        Affected::Some(names) => {
            let line = format!("{} scenes: {}", names.len(), names.join(", "));
            (Some(names), line)
        }
        Affected::All { unclaimed } if unclaimed.is_empty() => {
            let shared = refs.iter().find(|f| SHARED.contains(f)).copied();
            (None, format!("all (shared: {})", shared.unwrap_or("?")))
        }
        Affected::All { unclaimed } => (None, format!("all (unclaimed: {})", unclaimed.join(", "))),
    }
}
