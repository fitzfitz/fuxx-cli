//! What an agent can say about itself without a conversation (M11ak): its models, its slash
//! commands and its usage limits, read by short probe processes and kept per agent and folder
//! for the agent box's menus (M11p: a probe starts from a tab opening, a folder change or a menu
//! opening, never from drawing; there is no timer).

use crate::agent_process::{self, ProcessEvent};
use crate::providers::{Catalog, CommandKind, Probe, Provider, RateLimit, UsageGroup};
use futures::channel::mpsc;
use gpui::{App, BorrowAppContext, Global};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a usage reading is kept before the next menu opening reads it again.
pub const USAGE_FRESH: Duration = Duration::from_secs(60);
/// The longest a probe (or all of one list's probes together) may run before it is killed.
pub const PROBE_DEADLINE: Duration = Duration::from_secs(30);
/// How long a killed probe is waited for, so its process is gone when `run_probe` returns.
const REAP_WAIT: Duration = Duration::from_secs(2);

/// A list being read, read, or failed to read (the reason, for the log; the menu says
/// "Couldn't read the list").
#[derive(Clone, Debug, PartialEq)]
pub enum Lookup<T> {
    Loading,
    Ready(T),
    Failed(String),
}

/// What is known about each agent. A global, so a menu draws what is there and a tab (or a
/// finished probe) updates it; observers are told on every update.
#[derive(Default)]
pub struct Catalogs {
    catalogs: HashMap<(String, PathBuf), Lookup<Catalog>>,
    usage: HashMap<String, (Lookup<Vec<UsageGroup>>, Instant)>,
    limits: HashMap<String, Vec<RateLimit>>,
    versions: HashMap<String, Lookup<Option<String>>>,
}

impl Global for Catalogs {}

impl Catalogs {
    fn key(agent: &str, folder: &Path) -> (String, PathBuf) {
        (agent.to_string(), folder.to_path_buf())
    }

    #[allow(dead_code)] // read by the agent box's menus (a later M11ak task)
    pub fn catalog(&self, agent: &str, folder: &Path) -> Option<&Lookup<Catalog>> {
        self.catalogs.get(&Self::key(agent, folder))
    }

    /// Mark the catalog loading; false when it already is, or is read (a failure is tried again).
    pub fn begin_catalog(&mut self, agent: &str, folder: &Path) -> bool {
        let key = Self::key(agent, folder);
        if matches!(
            self.catalogs.get(&key),
            Some(Lookup::Loading | Lookup::Ready(_))
        ) {
            return false;
        }
        self.catalogs.insert(key, Lookup::Loading);
        true
    }

    pub fn set_catalog(&mut self, agent: &str, folder: &Path, found: Lookup<Catalog>) {
        self.catalogs.insert(Self::key(agent, folder), found);
    }

    /// The commands named in `skills` are skills (`Event::Skills`), not plain commands.
    pub fn mark_skills(&mut self, agent: &str, folder: &Path, skills: &[String]) {
        if let Some(Lookup::Ready(catalog)) = self.catalogs.get_mut(&Self::key(agent, folder)) {
            for command in &mut catalog.commands {
                if skills.contains(&command.name) {
                    command.kind = CommandKind::Skill;
                }
            }
        }
    }

    /// The usage as of `now`: None when there is none, or it is `USAGE_FRESH` old.
    pub fn usage(&self, agent: &str, now: Instant) -> Option<&Lookup<Vec<UsageGroup>>> {
        self.usage
            .get(agent)
            .filter(|(_, at)| now.saturating_duration_since(*at) < USAGE_FRESH)
            .map(|(found, _)| found)
    }

    /// Mark the usage loading; false when a fresh reading is loading or read (a failure, or a
    /// stale reading, is tried again).
    pub fn begin_usage(&mut self, agent: &str, now: Instant) -> bool {
        if matches!(
            self.usage(agent, now),
            Some(Lookup::Loading | Lookup::Ready(_))
        ) {
            return false;
        }
        self.usage.insert(agent.to_string(), (Lookup::Loading, now));
        true
    }

    pub fn set_usage(&mut self, agent: &str, found: Lookup<Vec<UsageGroup>>, now: Instant) {
        self.usage.insert(agent.to_string(), (found, now));
    }

    /// Keep what the agent said about a limit, in place of what it said about that limit before.
    pub fn note_limit(&mut self, agent: &str, limit: RateLimit) {
        let limits = self.limits.entry(agent.to_string()).or_default();
        match limits.iter_mut().find(|l| l.limit == limit.limit) {
            Some(known) => *known = limit,
            None => limits.push(limit),
        }
    }

    #[allow(dead_code)] // read by the agent box's menus (a later M11ak task)
    pub fn limits(&self, agent: &str) -> &[RateLimit] {
        self.limits.get(agent).map_or(&[], Vec::as_slice)
    }

    pub fn version(&self, agent: &str) -> Option<&Lookup<Option<String>>> {
        self.versions.get(agent)
    }

    pub fn set_version(&mut self, agent: &str, found: Lookup<Option<String>>) {
        self.versions.insert(agent.to_string(), found);
    }
}

/// Run `probe` in `cwd` (`PATH` = `path_env`): its stdout up to the line `until` accepts (or to
/// the end). Err on a spawn failure, a non-zero exit with nothing read, or past `deadline` (its
/// process group is killed, and waited for).
pub fn run_probe(
    probe: &Probe,
    cwd: &Path,
    path_env: Option<&str>,
    deadline: Duration,
) -> Result<String, String> {
    let (tx, mut rx) = mpsc::unbounded();
    let process = agent_process::spawn(&probe.argv, cwd, path_env, tx);
    for line in &probe.stdin {
        process.send(line.clone());
    }
    let started = Instant::now();
    let mut lines: Vec<String> = Vec::new();
    let finish = loop {
        if started.elapsed() >= deadline {
            break Err("it took too long".to_string());
        }
        match rx.try_recv() {
            Ok(ProcessEvent::Line(line)) => {
                let last = probe.until.is_some_and(|until| until(&line));
                lines.push(line);
                if last {
                    break Ok(());
                }
            }
            Ok(ProcessEvent::SpawnFailed(why)) => return Err(why),
            Ok(ProcessEvent::Exited { code, stderr_tail }) => {
                if lines.is_empty() && code != Some(0) {
                    let said = stderr_tail.trim();
                    break Err(if said.is_empty() {
                        format!(
                            "it exited ({})",
                            code.map_or("killed".into(), |c| c.to_string())
                        )
                    } else {
                        said.to_string()
                    });
                }
                break Ok(());
            }
            Ok(ProcessEvent::Ended { .. }) => {}
            Err(mpsc::TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => break Ok(()),
        }
    };
    if !process.reaped() {
        process.kill();
        let waited = Instant::now();
        while !process.reaped() && waited.elapsed() < REAP_WAIT {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    finish.map(|()| lines.join("\n"))
}

/// The login `PATH`, as a tab's boot reads it. Blocks the first time: background only.
fn login_path() -> Option<String> {
    crate::workspace_ssh::login_env().and_then(|env| {
        env.iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.clone())
    })
}

/// Update the cache and tell its observers.
fn with_catalogs<R>(cx: &mut App, f: impl FnOnce(&mut Catalogs) -> R) -> R {
    cx.update_default_global(|catalogs: &mut Catalogs, _| f(catalogs))
}

/// Put `found` for `agent` in `folder` into the cache (a tab's own process answered).
pub fn store_catalog(agent: &str, folder: &Path, found: Lookup<Catalog>, cx: &mut App) {
    with_catalogs(cx, |c| c.set_catalog(agent, folder, found));
}

/// Every probe of `provider`'s catalog, in `folder`, sharing one deadline. A probe that fails
/// leaves its output empty (agy's models stand without its commands); all failing is the
/// first one's reason.
fn read_catalog_of(
    provider: &dyn Provider,
    folder: &Path,
    path: Option<&str>,
) -> Result<Catalog, String> {
    let probes = provider.catalog_probes();
    if probes.is_empty() {
        return Err("no list".into());
    }
    let started = Instant::now();
    let mut outputs = Vec::new();
    let mut first_error = None;
    for probe in &probes {
        let left = PROBE_DEADLINE.saturating_sub(started.elapsed());
        match run_probe(probe, folder, path, left) {
            Ok(out) => outputs.push(out),
            Err(why) => {
                first_error.get_or_insert(why);
                outputs.push(String::new());
            }
        }
    }
    match first_error {
        Some(why) if outputs.iter().all(String::is_empty) => Err(why),
        _ => provider.read_catalog(&outputs),
    }
}

/// Read `provider`'s catalog for `folder` in the background (unless it is loading or read).
pub fn load_catalog(provider: &'static dyn Provider, folder: PathBuf, cx: &mut App) {
    let agent = provider.info().name;
    if !with_catalogs(cx, |c| c.begin_catalog(agent, &folder)) {
        return;
    }
    cx.spawn(async move |cx| {
        let probe_folder = folder.clone();
        let found = cx
            .background_executor()
            .spawn(async move { read_catalog_of(provider, &probe_folder, login_path().as_deref()) })
            .await;
        let found = match found {
            Ok(catalog) => Lookup::Ready(catalog),
            Err(why) => Lookup::Failed(why),
        };
        cx.update(|cx| store_catalog(agent, &folder, found, cx));
    })
    .detach();
}

/// Read `provider`'s usage limits in the background (unless a fresh reading is loading or read).
#[allow(dead_code)] // read by View Usage (a later M11ak task)
pub fn load_usage(provider: &'static dyn Provider, folder: PathBuf, cx: &mut App) {
    let agent = provider.info().name;
    let Some(probe) = provider.usage_probe() else {
        return;
    };
    if !with_catalogs(cx, |c| c.begin_usage(agent, Instant::now())) {
        return;
    }
    cx.spawn(async move |cx| {
        let found = cx
            .background_executor()
            .spawn(async move {
                run_probe(&probe, &folder, login_path().as_deref(), PROBE_DEADLINE)
                    .and_then(|out| provider.read_usage(&out))
            })
            .await;
        let found = match found {
            Ok(groups) => Lookup::Ready(groups),
            Err(why) => Lookup::Failed(why),
        };
        cx.update(|cx| with_catalogs(cx, |c| c.set_usage(agent, found, Instant::now())));
    })
    .detach();
}

/// Read the agent's version (`<program> --version`) in the background, once.
#[allow(dead_code)] // read by the agent box's menus (a later M11ak task)
pub fn load_version(provider: &'static dyn Provider, cx: &mut App) {
    let agent = provider.info().name;
    let started = with_catalogs(cx, |c| {
        let fresh = c.version(agent).is_none();
        if fresh {
            c.set_version(agent, Lookup::Loading);
        }
        fresh
    });
    if !started {
        return;
    }
    cx.spawn(async move |cx| {
        let found = cx
            .background_executor()
            .spawn(async move {
                let probe = Probe {
                    argv: vec![provider.program().to_string(), "--version".into()],
                    stdin: Vec::new(),
                    until: None,
                };
                let home = crate::workspace::home_dir().unwrap_or_else(|| PathBuf::from("/"));
                run_probe(&probe, &home, login_path().as_deref(), PROBE_DEADLINE)
            })
            .await;
        let found = match found {
            Ok(out) => Lookup::Ready(
                out.lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .map(String::from),
            ),
            Err(why) => Lookup::Failed(why),
        };
        cx.update(|cx| with_catalogs(cx, |c| c.set_version(agent, found)));
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{AgentCommand, LimitState};

    fn sh(script: &str) -> Probe {
        Probe {
            argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
            stdin: Vec::new(),
            until: None,
        }
    }

    fn command(name: &str) -> AgentCommand {
        AgentCommand {
            name: name.into(),
            about: String::new(),
            kind: CommandKind::Command,
            aliases: Vec::new(),
        }
    }

    fn limit(name: &str, state: LimitState) -> RateLimit {
        RateLimit {
            limit: name.into(),
            state,
            resets_at: None,
        }
    }

    #[test]
    fn one_probe_per_agent_and_folder() {
        let mut c = Catalogs::default();
        let a = Path::new("/a");
        assert!(c.begin_catalog("claude", a));
        assert!(!c.begin_catalog("claude", a), "loading already");
        c.set_catalog("claude", a, Lookup::Ready(Catalog::default()));
        assert!(!c.begin_catalog("claude", a), "ready already");
        assert!(c.begin_catalog("claude", Path::new("/b")));
        assert!(c.begin_catalog("agy", a));
        c.set_catalog("agy", a, Lookup::Failed("no".into()));
        assert!(c.begin_catalog("agy", a), "a failure is tried again");
        assert_eq!(c.catalog("agy", a), Some(&Lookup::Loading));
    }

    #[test]
    fn usage_is_kept_sixty_seconds() {
        let mut c = Catalogs::default();
        let t = Instant::now();
        assert!(c.usage("claude", t).is_none());
        assert!(c.begin_usage("claude", t));
        assert!(!c.begin_usage("claude", t), "loading already");
        c.set_usage("claude", Lookup::Ready(Vec::new()), t);
        assert!(c.usage("claude", t + Duration::from_secs(59)).is_some());
        assert!(!c.begin_usage("claude", t + Duration::from_secs(59)));
        assert!(c.usage("claude", t + Duration::from_secs(60)).is_none());
        assert!(c.begin_usage("claude", t + Duration::from_secs(60)));
    }

    #[test]
    fn a_rate_limit_replaces_its_own_limit_only() {
        let mut c = Catalogs::default();
        c.note_limit("claude", limit("Session", LimitState::Close));
        c.note_limit("claude", limit("Weekly", LimitState::Fine));
        c.note_limit("claude", limit("Session", LimitState::Reached));
        assert_eq!(
            c.limits("claude"),
            &[
                limit("Session", LimitState::Reached),
                limit("Weekly", LimitState::Fine)
            ]
        );
        assert!(c.limits("agy").is_empty());
    }

    #[test]
    fn mark_skills_turns_those_commands_into_skills() {
        let mut c = Catalogs::default();
        let f = Path::new("/a");
        c.set_catalog(
            "claude",
            f,
            Lookup::Ready(Catalog {
                commands: vec![command("help"), command("review")],
                ..Catalog::default()
            }),
        );
        c.mark_skills("claude", f, &["review".to_string()]);
        let Some(Lookup::Ready(catalog)) = c.catalog("claude", f) else {
            panic!("ready");
        };
        assert_eq!(catalog.commands[0].kind, CommandKind::Command);
        assert_eq!(catalog.commands[1].kind, CommandKind::Skill);
    }

    #[test]
    fn a_probe_reads_to_its_until_line() {
        fn is_b(line: &str) -> bool {
            line == "b"
        }
        let mut probe = sh("printf 'a\\nb\\nc\\n'");
        probe.until = Some(is_b);
        let out = run_probe(&probe, Path::new("/"), None, Duration::from_secs(5));
        assert_eq!(out, Ok("a\nb".to_string()));
        let all = run_probe(
            &sh("printf 'a\\nb\\n'"),
            Path::new("/"),
            None,
            Duration::from_secs(5),
        );
        assert_eq!(all, Ok("a\nb".to_string()));
    }

    #[test]
    fn a_failing_probe_with_nothing_read_fails() {
        let out = run_probe(
            &sh("echo oops >&2; exit 3"),
            Path::new("/"),
            None,
            Duration::from_secs(5),
        );
        assert_eq!(out, Err("oops".to_string()));
    }

    #[test]
    fn a_probe_past_its_deadline_is_killed_and_fails() {
        let pidfile = std::env::temp_dir().join(format!("fuxx-probe-pid-{}", std::process::id()));
        let probe = sh(&format!("echo $$ > {}; exec sleep 60", pidfile.display()));
        let started = Instant::now();
        let out = run_probe(&probe, Path::new("/"), None, Duration::from_millis(300));
        assert!(out.is_err(), "{out:?}");
        assert!(started.elapsed() < Duration::from_secs(3));
        let pid = std::fs::read_to_string(&pidfile).unwrap();
        let _ = std::fs::remove_file(&pidfile);
        let alive = std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(!alive, "the probe's process is gone");
    }

    #[test]
    fn a_missing_program_fails() {
        let probe = Probe {
            argv: vec!["fuxx-no-such-program".into()],
            stdin: Vec::new(),
            until: None,
        };
        assert!(run_probe(&probe, Path::new("/"), None, Duration::from_secs(5)).is_err());
    }
}
