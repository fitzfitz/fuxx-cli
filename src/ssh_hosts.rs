//! M10b: the hosts the SSH picker offers: `Host` names from `~/.ssh/config` (following
//! `Include`), the recently used hosts, and `ssh -G` to see what a host really resolves to.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::ssh_target::{Effective, SshTarget};

/// `Include` files followed at most this many levels deep (the config itself is level 0).
const MAX_INCLUDE_DEPTH: usize = 8;

/// Recently used hosts kept.
const MAX_RECENT: usize = 10;

/// `Host` names from `config`, in file order, following `Include` (relative paths are under
/// `home/.ssh`, `~` is `home`, `*` globs the file name). Wildcard and negated names are skipped,
/// duplicates dropped. A missing or unreadable file contributes nothing.
pub fn config_hosts(config: &Path, home: &Path) -> Vec<String> {
    let mut hosts = Vec::new();
    let mut stack = Vec::new();
    collect_hosts(config, home, 0, &mut stack, &mut hosts);
    hosts
}

fn collect_hosts(
    file: &Path,
    home: &Path,
    depth: usize,
    stack: &mut Vec<PathBuf>,
    hosts: &mut Vec<String>,
) {
    // Compare real paths, so a file reached through two spellings still counts as a cycle.
    let real = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    if stack.contains(&real) {
        return;
    }
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    stack.push(real);
    for line in text.lines() {
        let line = line.trim();
        let Some((keyword, rest)) = line.split_once(|c: char| c.is_whitespace() || c == '=') else {
            continue;
        };
        let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '=');
        match keyword.to_ascii_lowercase().as_str() {
            "host" => {
                for name in rest.split_whitespace() {
                    let name = name.trim_matches('"');
                    let skip =
                        name.is_empty() || name.starts_with('!') || name.contains(['*', '?']);
                    if !skip && !hosts.iter().any(|h| h == name) {
                        hosts.push(name.to_string());
                    }
                }
            }
            "include" if depth < MAX_INCLUDE_DEPTH => {
                for pattern in rest.split_whitespace() {
                    for path in include_paths(pattern.trim_matches('"'), home) {
                        collect_hosts(&path, home, depth + 1, stack, hosts);
                    }
                }
            }
            _ => {}
        }
    }
    stack.pop();
}

/// The files an `Include` pattern names: `~` is `home`, a relative path is under `home/.ssh`,
/// and `*` in the file name matches any run of characters (sorted, like ssh's glob).
fn include_paths(pattern: &str, home: &Path) -> Vec<PathBuf> {
    let full = if let Some(rest) = pattern.strip_prefix("~/") {
        home.join(rest)
    } else if Path::new(pattern).is_absolute() {
        PathBuf::from(pattern)
    } else {
        home.join(".ssh").join(pattern)
    };
    let Some(name) = full.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return Vec::new();
    };
    if !name.contains('*') {
        return vec![full];
    }
    let dir = full.parent().unwrap_or(Path::new("."));
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| glob_matches(&name, &e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    found
}

/// `*` matches any run of characters (including none); everything else matches itself.
fn glob_matches(pattern: &str, text: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == text,
        Some((head, tail)) => {
            let Some(rest) = text.strip_prefix(head) else {
                return false;
            };
            (0..=rest.len())
                .filter(|&i| rest.is_char_boundary(i))
                .any(|i| glob_matches(tail, &rest[i..]))
        }
    }
}

/// A host the user connected to, and when (seconds since the epoch).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub target: SshTarget,
    pub used_at: u64,
}

/// The recent hosts, newest first. A missing or broken file reads as empty.
pub fn load_recent(path: &Path) -> Vec<Recent> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Put `target` first, replacing any earlier entry for the same host, keep the newest ten,
/// and write the file.
pub fn record_recent(path: &Path, target: &SshTarget, now: u64) -> io::Result<()> {
    let mut list = load_recent(path);
    list.retain(|r| r.target.host != target.host);
    list.insert(
        0,
        Recent {
            target: target.clone(),
            used_at: now,
        },
    );
    list.truncate(MAX_RECENT);
    let text = serde_json::to_string_pretty(&list).map_err(io::Error::other)?;
    crate::settings::write_atomic(path, &(text + "\n"))
}

/// What the picker lists: recent hosts first, then the config hosts not already shown and not
/// git remotes. A host whose `ssh -G` failed (`None`) is still shown.
pub fn picker_rows(recent: &[Recent], config: &[(String, Option<Effective>)]) -> Vec<SshTarget> {
    let mut rows: Vec<SshTarget> = recent.iter().map(|r| r.target.clone()).collect();
    for (host, effective) in config {
        let hidden = effective
            .as_ref()
            .is_some_and(crate::ssh_target::is_git_host);
        if !hidden && !rows.iter().any(|t| &t.host == host) {
            rows.push(SshTarget {
                host: host.clone(),
                tmux: false,
            });
        }
    }
    rows
}

/// `resolve_with` using the `ssh` on the PATH.
pub fn resolve(host: &str, timeout: Duration) -> Option<Effective> {
    resolve_with(OsStr::new("ssh"), host, timeout)
}

/// Run `program -G -- host` and parse what it prints. `None` if it can't start, exits
/// non-zero, or is still running after `timeout` (it is killed then).
pub fn resolve_with(program: &OsStr, host: &str, timeout: Duration) -> Option<Effective> {
    let mut command = std::process::Command::new(program);
    command.args(["-G", "--", host]);
    let (status, text) = run_capped(&mut command, timeout)?;
    status
        .success()
        .then(|| crate::ssh_target::parse_effective(&text))
}

/// Run `command` with no stdin and its stderr dropped, and collect its stdout. `None` if it
/// can't start or is still running after `timeout` (it is killed and reaped then).
pub(crate) fn run_capped(
    command: &mut std::process::Command,
    timeout: Duration,
) -> Option<(std::process::ExitStatus, String)> {
    use std::io::Read;
    use std::process::Stdio;

    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // Read on a thread so a full pipe can't stall the child while we watch the clock.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let text = rx.recv_timeout(Duration::from_secs(1)).ok()?;
    Some((status, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fuxx-ssh-hosts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ssh")).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn target(host: &str) -> SshTarget {
        SshTarget {
            host: host.to_string(),
            tmux: false,
        }
    }

    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        write(&path, &format!("#!/bin/sh\n{body}\n"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn hosts_follow_includes_and_skip_patterns() {
        let home = temp("includes");
        let ssh = home.join(".ssh");
        write(
            &ssh.join("config"),
            "Include conf.d/*.conf\nInclude ~/extra/more\n\
             HOST devbox Build\n  HostName x\nhost * !bad wild?\nHost devbox\nhost=eq\n\
             Match all\nHost last\n",
        );
        write(&ssh.join("conf.d/a.conf"), "Host from-a\n");
        write(&ssh.join("conf.d/b.conf"), "Host from-b\n");
        write(&ssh.join("conf.d/skip.txt"), "Host skipped\n");
        write(&home.join("extra/more"), "Host from-tilde\n");

        assert_eq!(
            config_hosts(&ssh.join("config"), &home),
            [
                "from-a",
                "from-b",
                "from-tilde",
                "devbox",
                "Build",
                "eq",
                "last"
            ]
        );
        assert!(config_hosts(&ssh.join("missing"), &home).is_empty());
    }

    #[test]
    fn include_cycles_and_depth_are_bounded() {
        let home = temp("cycles");
        let ssh = home.join(".ssh");
        write(&ssh.join("config"), "Host top\nInclude loop\n");
        write(
            &ssh.join("loop"),
            "Host in-loop\nInclude config\nInclude loop\n",
        );
        assert_eq!(config_hosts(&ssh.join("config"), &home), ["top", "in-loop"]);

        // A chain of 11 files: the config is level 0, includes are followed 8 levels deep.
        let home = temp("depth");
        let ssh = home.join(".ssh");
        for i in 0..=10 {
            write(
                &ssh.join(format!("f{i}")),
                &format!("Host h{i}\nInclude f{}\n", i + 1),
            );
        }
        let hosts = config_hosts(&ssh.join("f0"), &home);
        assert_eq!(hosts.len(), 9);
        assert_eq!(hosts.last().unwrap(), "h8");
    }

    #[test]
    fn recent_is_newest_first_deduped_and_capped() {
        let dir = temp("recent");
        let file = dir.join("fuxx/ssh-recent.json");
        assert!(load_recent(&file).is_empty());

        record_recent(&file, &target("a"), 1).unwrap();
        record_recent(&file, &target("b"), 2).unwrap();
        let tmux_a = SshTarget {
            host: "a".into(),
            tmux: true,
        };
        record_recent(&file, &tmux_a, 3).unwrap();
        assert_eq!(
            load_recent(&file),
            vec![
                Recent {
                    target: tmux_a,
                    used_at: 3
                },
                Recent {
                    target: target("b"),
                    used_at: 2
                },
            ]
        );

        for i in 0..15 {
            record_recent(&file, &target(&format!("h{i}")), 10 + i).unwrap();
        }
        let list = load_recent(&file);
        assert_eq!(list.len(), 10);
        assert_eq!(list[0].target.host, "h14");
        assert_eq!(list[9].target.host, "h5");
    }

    #[test]
    fn broken_recent_file_reads_empty() {
        let dir = temp("broken");
        let file = dir.join("ssh-recent.json");
        write(&file, "{ not json");
        assert!(load_recent(&file).is_empty());
        // Recording over it starts a fresh list.
        record_recent(&file, &target("a"), 1).unwrap();
        assert_eq!(load_recent(&file).len(), 1);
    }

    #[test]
    fn picker_rows_put_recent_first_and_hide_git_hosts() {
        let recent = vec![
            Recent {
                target: SshTarget {
                    host: "b".into(),
                    tmux: true,
                },
                used_at: 2,
            },
            Recent {
                target: target("github-recent"),
                used_at: 1,
            },
        ];
        let plain = Effective::default();
        let git = Effective {
            user: Some("git".into()),
            ..Effective::default()
        };
        let config = vec![
            ("a".to_string(), Some(plain.clone())),
            ("b".to_string(), Some(plain.clone())),
            ("gh".to_string(), Some(git)),
            ("unresolved".to_string(), None),
        ];
        let rows = picker_rows(&recent, &config);
        let names: Vec<&str> = rows.iter().map(|t| t.host.as_str()).collect();
        // Recent hosts stay even if they look like git hosts; config hosts add the rest.
        assert_eq!(names, ["b", "github-recent", "a", "unresolved"]);
        assert!(rows[0].tmux);
    }

    #[test]
    fn resolve_uses_a_fake_ssh() {
        let dir = temp("fake");
        let fake = script(
            &dir,
            "ssh",
            // The host must arrive after `-G --`.
            "[ \"$1 $2 $3\" = \"-G -- devbox\" ] || exit 3\n\
             printf 'user alice\\nhostname example.com\\ncontrolmaster false\\nserveraliveinterval 0\\n'",
        );
        let e = resolve_with(fake.as_os_str(), "devbox", Duration::from_secs(2)).unwrap();
        assert_eq!(e.user.as_deref(), Some("alice"));
        assert_eq!(e.hostname.as_deref(), Some("example.com"));
        assert!(!e.has_control_master && !e.has_keepalive);
        // A failing ssh, or one that doesn't exist, gives None.
        assert!(resolve_with(fake.as_os_str(), "other", Duration::from_secs(2)).is_none());
        assert!(
            resolve_with(
                OsStr::new("/nonexistent/ssh"),
                "devbox",
                Duration::from_secs(1)
            )
            .is_none()
        );
    }

    #[test]
    fn resolve_times_out() {
        let dir = temp("slow");
        let slow = script(&dir, "ssh", "sleep 5");
        let start = Instant::now();
        assert!(resolve_with(slow.as_os_str(), "devbox", Duration::from_millis(200)).is_none());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
