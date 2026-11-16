//! `--changed <base>`: which scenes a git change touches, and the line that says so.

use std::process::Command;

use super::{Affected, SHARED, Scene, affected};

/// `git <args>` in the repo's top folder (`-C`, so a run from a subfolder sees the same files),
/// paths unquoted and NUL-separated, as the lines of paths it printed.
fn git(top: &str, args: &[&str]) -> Result<Vec<String>, String> {
    let mut command = Command::new("git");
    command
        .args(["-C", top, "-c", "core.quotePath=false"])
        .args(args);
    let out = command
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
    Ok(out
        .stdout
        .split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect())
}

/// The repo-relative files changed since `base` (`git diff <base>...HEAD`) or not yet committed
/// (staged, unstaged, untracked). Renames count as a delete and an add, so a file moved out of
/// `src/` still shows. Err names what git said when it fails or doesn't know `base`.
pub(super) fn changed_files(base: &str) -> Result<Vec<String>, String> {
    let top = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("git did not run: {e}"))?;
    if !top.status.success() {
        return Err("not in a git repository".into());
    }
    let top = String::from_utf8_lossy(&top.stdout).trim().to_string();
    let range = format!("{base}...HEAD");
    let mut files: Vec<String> = Vec::new();
    for args in [
        vec!["diff", "-z", "--name-only", "--no-renames", range.as_str()],
        vec!["diff", "-z", "--name-only", "--no-renames", "HEAD"],
        vec![
            "ls-files",
            "-z",
            "--others",
            "--exclude-standard",
            "--full-name",
        ],
    ] {
        for file in git(&top, &args)? {
            if !files.contains(&file) {
                files.push(file);
            }
        }
    }
    Ok(files)
}

/// "1 scene", "13 scenes".
fn scenes_word(n: usize) -> String {
    format!("{n} scene{}", if n == 1 { "" } else { "s" })
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
            let line = format!("{}: {}", scenes_word(names.len()), names.join(", "));
            (Some(names), line)
        }
        Affected::All { shared, unclaimed } => {
            let mut why = Vec::new();
            if !shared.is_empty() {
                why.push(format!("shared: {}", shared.join(", ")));
            }
            if !unclaimed.is_empty() {
                why.push(format!("unclaimed: {}", unclaimed.join(", ")));
            }
            (None, format!("all ({})", why.join("; ")))
        }
    }
}

/// `decide`'s answer narrowed by a name `filter` (the run's own): the scenes left (`None` while
/// there is no filter and the answer is all) and the line, which then says what the filter kept.
pub(super) fn with_filter(
    only: Option<Vec<&'static str>>,
    line: String,
    scenes: &[Scene],
    filter: Option<&str>,
) -> (Option<Vec<&'static str>>, String) {
    let Some(filter) = filter else {
        return (only, line);
    };
    if only.as_ref().is_some_and(|n| n.is_empty()) {
        return (only, line);
    }
    let kept: Vec<&'static str> = scenes
        .iter()
        .map(|s| s.name)
        .filter(|n| only.as_ref().is_none_or(|o| o.contains(n)) && n.contains(filter))
        .collect();
    let line = format!(
        "{line}; filter {filter:?} keeps {}",
        if kept.is_empty() {
            "none".to_string()
        } else {
            format!("{}: {}", scenes_word(kept.len()), kept.join(", "))
        }
    );
    (Some(kept), line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenes() -> Vec<Scene> {
        super::super::registry::claiming()
    }

    #[test]
    fn the_line_says_shared_and_unclaimed_and_counts_one_scene_in_the_singular() {
        let files = Ok(vec!["src/theme.rs".to_string(), "src/foo.rs".to_string()]);
        let (only, line) = decide(files, &scenes());
        assert_eq!(only, None);
        assert_eq!(line, "all (shared: src/theme.rs; unclaimed: src/foo.rs)");
        let (only, line) = decide(Ok(vec!["src/ui_shots/branch.rs".into()]), &scenes());
        let n = only.expect("some").len();
        assert!(line.starts_with(&scenes_word(n)), "{line}");
        assert_eq!(scenes_word(1), "1 scene");
        assert_eq!(scenes_word(2), "2 scenes");
        let (only, line) = decide(Err("git failed".into()), &scenes());
        assert_eq!((only, line.as_str()), (None, "all (git failed)"));
    }

    #[test]
    fn a_filter_narrows_the_line_to_the_true_intersection() {
        let all = scenes();
        let (only, line) = decide(Ok(vec!["src/file_tab.rs".into()]), &all);
        let (kept, line) = with_filter(only, line, &all, Some("file-tab-md"));
        let kept = kept.expect("some");
        assert!(kept.iter().all(|n| n.contains("file-tab-md")) && !kept.is_empty());
        assert!(line.contains("filter \"file-tab-md\" keeps"), "{line}");
        let (none, _) = with_filter(Some(vec!["smoke"]), String::new(), &all, Some("zzz"));
        assert_eq!(none, Some(vec![]));
    }
}
