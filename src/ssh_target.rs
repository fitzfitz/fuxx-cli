//! M10b: the `ssh` command line for an SSH workspace. Pure: no process, no filesystem.
//!
//! The host always travels as one argv element after `--`, never through a shell. Connection
//! reuse and keepalive are added only when the user's own ssh config leaves them unset, judged
//! from the output of `ssh -G HOST` (parsed by `parse_effective`).

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Where a session connects: a host as ssh understands it (`user@host`, or an alias from
/// `~/.ssh/config`), and whether the remote side runs inside tmux.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SshTarget {
    pub host: String,
    #[serde(default)]
    pub tmux: bool,
    /// The session's own tmux session name (after `fuxx-`), made once when it opens, so each
    /// pane has its own and a reconnect or restore comes back to it. Never kept for recent
    /// hosts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmux_name: Option<String>,
}

/// Trim `host` and refuse anything ssh could read as an option or that has no business in a
/// host: empty, a leading `-`, whitespace or control characters inside. Shell characters are
/// fine, since no shell ever sees the host.
pub fn validate_host(host: &str) -> Result<String, String> {
    let host = host.trim();
    if host.is_empty() {
        return Err("the host is empty".to_string());
    }
    if host.starts_with('-') {
        return Err("a host cannot start with '-'".to_string());
    }
    if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("a host cannot contain spaces or control characters".to_string());
    }
    Ok(host.to_string())
}

/// The few settings from `ssh -G` that decide what we add to the command line.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Effective {
    pub user: Option<String>,
    pub hostname: Option<String>,
    /// The user's config already turns on connection sharing.
    pub has_control_master: bool,
    /// The user's config already sets a keepalive interval.
    pub has_keepalive: bool,
}

/// Read `ssh -G` output: lowercase `key value` lines. A missing key counts as unset.
pub fn parse_effective(ssh_g_output: &str) -> Effective {
    let mut e = Effective::default();
    for line in ssh_g_output.lines() {
        let Some((key, value)) = line.trim().split_once(char::is_whitespace) else {
            continue;
        };
        let value = value.trim();
        match key {
            "user" => e.user = Some(value.to_string()),
            "hostname" => e.hostname = Some(value.to_string()),
            "controlmaster" => e.has_control_master = !matches!(value, "false" | "no"),
            "serveraliveinterval" => e.has_keepalive = value != "0",
            _ => {}
        }
    }
    e
}

/// Hosts that are git remotes, not machines to open a terminal on. They stay out of the picker.
pub fn is_git_host(e: &Effective) -> bool {
    const GIT_HOSTNAMES: [&str; 5] = [
        "github.com",
        "gitlab.com",
        "bitbucket.org",
        "ssh.dev.azure.com",
        "vs-ssh.visualstudio.com",
    ];
    e.user.as_deref() == Some("git")
        || e.hostname
            .as_deref()
            .is_some_and(|h| GIT_HOSTNAMES.iter().any(|g| g.eq_ignore_ascii_case(h)))
}

/// A tmux-safe session name: only `[A-Za-z0-9_-]`, at most 32 characters, `session` if empty.
pub fn tmux_session_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    if cleaned.is_empty() {
        "session".to_string()
    } else {
        cleaned
    }
}

/// The full argv to run as the pane's program. Connection sharing needs a `control_dir` and a
/// config that doesn't already share; keepalive needs a config without an interval.
pub fn ssh_argv(
    t: &SshTarget,
    name: &str,
    e: &Effective,
    control_dir: Option<&Path>,
) -> Vec<String> {
    let mut argv = vec!["ssh".to_string(), "-t".to_string()];
    let mut option = |text: String| {
        argv.push("-o".to_string());
        argv.push(text);
    };
    if let Some(dir) = control_dir.filter(|_| !e.has_control_master) {
        option("ControlMaster=auto".to_string());
        option(format!("ControlPath={}/%C", dir.display()));
        option("ControlPersist=10m".to_string());
    }
    if !e.has_keepalive {
        option("ServerAliveInterval=20".to_string());
        option("ServerAliveCountMax=2".to_string());
    }
    argv.push("--".to_string());
    argv.push(t.host.clone());
    if t.tmux {
        argv.extend(
            ["tmux", "new", "-A", "-s"]
                .map(String::from)
                .into_iter()
                .chain([format!("fuxx-{}", tmux_session_name(name))]),
        );
    }
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    // Recorded from `ssh -G` (trimmed to the lines that matter plus a few neighbours).
    const PLAIN: &str = "host devbox\nuser alice\nhostname example.com\nport 22\n\
        controlmaster false\ncontrolpersist no\nserveralivecountmax 3\nserveraliveinterval 0\n";
    const CONFIGURED: &str = "host build\nuser bob\nhostname build.internal\nport 22\n\
        controlmaster auto\ncontrolpath /home/bob/.ssh/cm-%C\ncontrolpersist 600\n\
        serveralivecountmax 3\nserveraliveinterval 30\n";

    fn target(host: &str, tmux: bool) -> SshTarget {
        SshTarget {
            host: host.to_string(),
            tmux,
            tmux_name: None,
        }
    }

    fn v(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn validate_host_refuses_options_and_control_text() {
        for bad in ["", "   ", "-oProxyCommand=x", "a b", "h\nx", "h\tx"] {
            assert!(validate_host(bad).is_err(), "{bad:?} should be refused");
        }
        assert_eq!(validate_host("h;rm"), Ok("h;rm".to_string()));
        assert_eq!(validate_host(" user@host "), Ok("user@host".to_string()));
    }

    #[test]
    fn parse_effective_reads_recorded_output() {
        assert_eq!(
            parse_effective(PLAIN),
            Effective {
                user: Some("alice".into()),
                hostname: Some("example.com".into()),
                has_control_master: false,
                has_keepalive: false,
            }
        );
        assert_eq!(
            parse_effective(CONFIGURED),
            Effective {
                user: Some("bob".into()),
                hostname: Some("build.internal".into()),
                has_control_master: true,
                has_keepalive: true,
            }
        );
        assert_eq!(parse_effective(""), Effective::default());
    }

    #[test]
    fn git_hosts_are_hidden() {
        let git_user = parse_effective("user git\nhostname code.example.com\n");
        let github = parse_effective("user alice\nhostname github.com\n");
        assert!(is_git_host(&git_user));
        assert!(is_git_host(&github));
        assert!(is_git_host(&parse_effective("hostname GitHub.com\n")));
        assert!(!is_git_host(&parse_effective(PLAIN)));
    }

    #[test]
    fn tmux_names_are_safe() {
        assert_eq!(tmux_session_name("my agent!"), "my_agent_");
        assert_eq!(tmux_session_name(&"a".repeat(40)), "a".repeat(32));
        assert_eq!(tmux_session_name(""), "session");
    }

    #[test]
    fn argv_adds_reuse_and_keepalive_only_when_unset() {
        let dir = Path::new("/tmp/fuxx-501");
        let plain = parse_effective(PLAIN);
        let configured = parse_effective(CONFIGURED);

        assert_eq!(
            ssh_argv(&target("devbox", false), "n", &plain, Some(dir)),
            v(&[
                "ssh",
                "-t",
                "-o",
                "ControlMaster=auto",
                "-o",
                "ControlPath=/tmp/fuxx-501/%C",
                "-o",
                "ControlPersist=10m",
                "-o",
                "ServerAliveInterval=20",
                "-o",
                "ServerAliveCountMax=2",
                "--",
                "devbox"
            ])
        );
        assert_eq!(
            ssh_argv(&target("devbox", true), "my agent!", &plain, Some(dir)),
            v(&[
                "ssh",
                "-t",
                "-o",
                "ControlMaster=auto",
                "-o",
                "ControlPath=/tmp/fuxx-501/%C",
                "-o",
                "ControlPersist=10m",
                "-o",
                "ServerAliveInterval=20",
                "-o",
                "ServerAliveCountMax=2",
                "--",
                "devbox",
                "tmux",
                "new",
                "-A",
                "-s",
                "fuxx-my_agent_"
            ])
        );
        // No shared-connection folder: keepalive only.
        assert_eq!(
            ssh_argv(&target("devbox", false), "n", &plain, None),
            v(&[
                "ssh",
                "-t",
                "-o",
                "ServerAliveInterval=20",
                "-o",
                "ServerAliveCountMax=2",
                "--",
                "devbox"
            ])
        );
        // Everything already configured: nothing added.
        assert_eq!(
            ssh_argv(&target("build", false), "n", &configured, Some(dir)),
            v(&["ssh", "-t", "--", "build"])
        );
        assert_eq!(
            ssh_argv(&target("build", true), "", &configured, Some(dir)),
            v(&[
                "ssh",
                "-t",
                "--",
                "build",
                "tmux",
                "new",
                "-A",
                "-s",
                "fuxx-session"
            ])
        );
    }
}
