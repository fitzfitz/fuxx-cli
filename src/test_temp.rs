//! Scratch folders for tests and ui-shots scenes. Each lives in this run's own folder,
//! `$TMPDIR/fuxx-t/<pid>`, which is removed when the run exits; a run that dies (killed,
//! aborted) leaves its folder for the next run to sweep. The parent name is short because a
//! Unix socket path is capped at 104 bytes on macOS and some tests bind one in here.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The folder all runs' folders sit in.
const PARENT: &str = "fuxx-t";

/// The run's folder that set its removal at exit, and the process it belongs to: a forked
/// child that exits before its `exec` must not remove its parent's folder.
static RUN: OnceLock<(PathBuf, u32)> = OnceLock::new();

unsafe extern "C" {
    fn atexit(f: extern "C" fn()) -> std::os::raw::c_int;
    fn kill(pid: i32, sig: i32) -> std::os::raw::c_int;
}

/// A path named `name` inside this run's folder (the folder exists; `name` doesn't, yet).
pub fn dir(name: &str) -> PathBuf {
    run().join(name)
}

/// This run's folder: made on first use, with the dead runs' folders swept and its own
/// removal set for exit.
fn run() -> &'static Path {
    let (root, _) = RUN.get_or_init(|| {
        let parent = std::env::temp_dir().join(PARENT);
        sweep(&parent);
        let root = parent.join(std::process::id().to_string());
        std::fs::create_dir_all(&root).expect("make the run's test folder");
        // SAFETY: `remove_run` is a plain `extern "C" fn` with no captured state.
        unsafe { atexit(remove_run) };
        (root, std::process::id())
    });
    root
}

extern "C" fn remove_run() {
    if let Some((root, owner)) = RUN.get() {
        remove_if_owner(root, *owner, std::process::id());
    }
}

/// Removes `root` when the exiting process is the one that made it.
fn remove_if_owner(root: &Path, owner: u32, me: u32) {
    if owner == me {
        let _ = std::fs::remove_dir_all(root);
    }
}

/// Removes the folders of runs whose process is gone. Anything not named by a pid is left.
fn sweep(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<i32>().ok())
        else {
            continue;
        };
        if pid > 0 && !alive(pid) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Whether process `pid` exists (one owned by someone else counts).
fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that `pid` exists; nothing is sent.
    let r = unsafe { kill(pid, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() != Some(ESRCH)
}

/// "No such process", the same on macOS and Linux.
const ESRCH: i32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    /// A pid that just ended: a child that exited and was reaped.
    fn dead_pid() -> i32 {
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        child.wait().unwrap();
        child.id() as i32
    }

    #[test]
    fn a_test_folder_is_inside_this_runs_folder() {
        let path = dir("fuxx-test-temp-here");
        let run = std::env::temp_dir()
            .join(PARENT)
            .join(std::process::id().to_string());
        assert_eq!(path, run.join("fuxx-test-temp-here"));
        assert!(run.is_dir(), "the run's folder is made on first use");
    }

    #[test]
    fn a_dead_runs_folder_is_swept_and_a_live_ones_kept() {
        let parent = dir("fuxx-test-temp-sweep");
        let mut live = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let dead = parent.join(dead_pid().to_string());
        let kept = parent.join(live.id().to_string());
        let other = parent.join("not-a-pid");
        for d in [&dead, &kept, &other] {
            std::fs::create_dir_all(d.join("inside")).unwrap();
        }
        sweep(&parent);
        let _ = live.kill();
        let _ = live.wait();
        assert!(!dead.exists(), "a dead run's folder goes");
        assert!(kept.exists(), "a live run's folder stays");
        assert!(other.exists(), "a folder not named by a pid stays");
    }

    #[test]
    fn only_the_process_that_made_a_run_folder_removes_it() {
        let root = dir("fuxx-test-temp-owner");
        std::fs::create_dir_all(root.join("inside")).unwrap();
        remove_if_owner(&root, 1, 2);
        assert!(root.exists(), "a forked child leaves it");
        remove_if_owner(&root, 2, 2);
        assert!(!root.exists(), "its own run removes it");
    }

    /// Test scratch goes through `dir` so it is removed with the run; these are the only
    /// places that may name the system temp folder.
    #[test]
    fn tests_make_their_folders_in_the_run_folder() {
        const ALLOWED: &[&str] = &[
            "src/test_temp.rs",
            // A screenshot's default spot, and its parse test.
            "src/workspace_browser.rs",
            // The browser probe's screenshot, kept for a person to look at.
            "src/browser/probe.rs",
            // Paths only: nothing is made there.
            "src/edit_ledger.rs",
            "src/workspace_api.rs",
            "src/workspace.rs",
            "src/ui_shots/agent.rs",
        ];
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stray = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let rel = format!("src/{}", path.strip_prefix(&src).unwrap().display());
                if !rel.ends_with(".rs") || ALLOWED.contains(&rel.as_str()) {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (i, line) in text.lines().enumerate() {
                    // The app's own pasted-image folder (a scene checks a paste lands there).
                    let pasted = line.contains("PASTED_DIR") || line.contains("\"fuxx-images\"");
                    if line.contains("temp_dir()") && !pasted {
                        stray.push(format!("{rel}:{}", i + 1));
                    }
                }
            }
        }
        assert!(stray.is_empty(), "use crate::test_temp::dir: {stray:#?}");
    }
}
