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

/// How often the usage is read again in the background while the window is seen (M11ap).
pub const USAGE_REFRESH: Duration = Duration::from_secs(5 * 60);

/// Whether background usage reads are wanted at all: the window seen and the status bar (which
/// shows them) on (the review's finding 4).
pub fn reads_wanted(window_seen: bool, bar_shown: bool) -> bool {
    window_seen && bar_shown
}

/// The line under kept numbers whose refresh failed: `Couldn't refresh · updated 12m ago`
/// (none when the last read worked).
pub fn stale_note(failed: bool, ago_secs: Option<u64>) -> Option<String> {
    let ago = ago_secs?;
    failed.then(|| match ago {
        0..60 => "Couldn't refresh \u{b7} updated just now".to_string(),
        s => format!("Couldn't refresh \u{b7} updated {}m ago", s / 60),
    })
}

/// Whether the background usage read is due: never read, or read `USAGE_REFRESH` ago, and the
/// window is seen (a hidden window reads nothing).
pub fn refresh_due(last: Option<Instant>, now: Instant, seen: bool) -> bool {
    seen && last.is_none_or(|at| now.saturating_duration_since(at) >= USAGE_REFRESH)
}
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
    /// When each limit was last said (by agent and limit name): a live value newer than the
    /// last `/usage` reading wins over it (M11ap).
    limits_seen: HashMap<(String, String), Instant>,
    /// The last reading that worked, and when: kept while the next one is read or fails, so
    /// numbers never blank out (M11ap).
    usage_ready: HashMap<String, (Vec<UsageGroup>, Instant)>,
    known_folders: HashMap<String, Lookup<Vec<PathBuf>>>,
}

impl Global for Catalogs {}

impl Catalogs {
    fn key(agent: &str, folder: &Path) -> (String, PathBuf) {
        (agent.to_string(), folder.to_path_buf())
    }

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

    /// A catalog still loading becomes failed (its asker's process ended, or the tab closed,
    /// without an answer); true if it was. One read, or never asked, is left alone.
    pub fn fail_catalog_if_loading(&mut self, agent: &str, folder: &Path, why: &str) -> bool {
        match self.catalogs.get_mut(&Self::key(agent, folder)) {
            Some(found @ Lookup::Loading) => {
                *found = Lookup::Failed(why.to_string());
                true
            }
            _ => false,
        }
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

    /// The usage last read (or being read), however old: what the open View Usage panel keeps
    /// showing. `usage` says whether it is still fresh.
    pub fn usage_last(&self, agent: &str) -> Option<&Lookup<Vec<UsageGroup>>> {
        self.usage.get(agent).map(|(found, _)| found)
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

    /// When the last good reading came (the popover's "Updated … ago").
    pub fn usage_ready_at(&self, agent: &str) -> Option<Instant> {
        self.usage_ready.get(agent).map(|(_, at)| *at)
    }

    /// When the usage was last asked for or read (whatever came of it).
    pub fn usage_read_at(&self, agent: &str) -> Option<Instant> {
        self.usage.get(agent).map(|(_, at)| *at)
    }

    pub fn set_usage(&mut self, agent: &str, found: Lookup<Vec<UsageGroup>>, now: Instant) {
        if let Lookup::Ready(groups) = &found {
            self.usage_ready
                .insert(agent.to_string(), (groups.clone(), now));
        }
        self.usage.insert(agent.to_string(), (found, now));
    }

    /// Keep what the agent said about a limit, in place of what it said about that limit before.
    pub fn note_limit(&mut self, agent: &str, limit: RateLimit) {
        self.note_limit_at(agent, limit, Instant::now());
    }

    /// `note_limit`, said `now`.
    pub fn note_limit_at(&mut self, agent: &str, limit: RateLimit, now: Instant) {
        self.limits_seen
            .insert((agent.to_string(), limit.limit.clone()), now);
        let limits = self.limits.entry(agent.to_string()).or_default();
        match limits.iter_mut().find(|l| l.limit == limit.limit) {
            Some(known) => *known = limit,
            None => limits.push(limit),
        }
    }

    pub fn limits(&self, agent: &str) -> &[RateLimit] {
        self.limits.get(agent).map_or(&[], Vec::as_slice)
    }

    /// The agent's usage now (M11ap): the last `/usage` reading, with each live window the agent
    /// said since (Claude's stream) in place of its limit; with no reading yet, the live windows
    /// alone. None when there is neither.
    pub fn live_usage(&self, agent: &str) -> Option<Vec<UsageGroup>> {
        self.live_usage_at(agent, crate::agent_box::now_unix())
    }

    /// `live_usage` at `now_unix`: a limit whose reset has passed counts as unused (its last
    /// number is from before the reset; the review's finding 5).
    pub fn live_usage_at(&self, agent: &str, now_unix: i64) -> Option<Vec<UsageGroup>> {
        use crate::providers::{Reset, UsageLimit};
        let (mut groups, read_at) = match self.usage_ready.get(agent) {
            Some((groups, at)) => (groups.clone(), Some(*at)),
            None => (Vec::new(), None),
        };
        for limit in self.limits(agent) {
            let Some(used) = limit.used_pct else {
                continue;
            };
            let seen = self
                .limits_seen
                .get(&(agent.to_string(), limit.limit.clone()))
                .copied();
            if read_at.is_some_and(|read| seen.is_none_or(|seen| seen <= read)) {
                continue;
            }
            let fresh = UsageLimit::Read {
                name: limit.limit.clone(),
                left_pct: 100 - used.min(100),
                resets: limit.resets_at.map(Reset::At),
            };
            let known = groups
                .iter_mut()
                .flat_map(|g| g.limits.iter_mut())
                .find(|l| matches!(l, UsageLimit::Read { name, .. } if *name == limit.limit));
            match known {
                Some(l) => *l = fresh,
                None => match groups.first_mut() {
                    Some(g) => g.limits.push(fresh),
                    None => groups.push(UsageGroup {
                        name: String::new(),
                        about: None,
                        limits: vec![fresh],
                    }),
                },
            }
        }
        for limit in groups.iter_mut().flat_map(|g| g.limits.iter_mut()) {
            if let UsageLimit::Read {
                left_pct, resets, ..
            } = limit
                && matches!(resets, Some(Reset::At(at)) if *at <= now_unix)
            {
                *left_pct = 100;
                *resets = None;
            }
        }
        (!groups.is_empty()).then_some(groups)
    }

    /// The folders the agent itself knows (its own project list), once read.
    pub fn known_folders(&self, agent: &str) -> Option<&Lookup<Vec<PathBuf>>> {
        self.known_folders.get(agent)
    }

    pub fn set_known_folders(&mut self, agent: &str, found: Lookup<Vec<PathBuf>>) {
        self.known_folders.insert(agent.to_string(), found);
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
    run_probe_stoppable(
        probe,
        cwd,
        path_env,
        deadline,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

/// `run_probe`, stopped (its process group killed) as soon as `stop` is set: Err("stopped")
/// (M11aq: ✦'s Esc).
pub fn run_probe_stoppable(
    probe: &Probe,
    cwd: &Path,
    path_env: Option<&str>,
    deadline: Duration,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<String, String> {
    let (tx, mut rx) = mpsc::unbounded();
    let process = agent_process::spawn(&probe.argv, cwd, path_env, tx);
    for line in &probe.stdin {
        process.send(line.clone());
    }
    let started = Instant::now();
    let mut lines: Vec<String> = Vec::new();
    let finish = loop {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            break Err("stopped".to_string());
        }
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
pub(crate) fn login_path() -> Option<String> {
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

/// Fail `agent`'s catalog for `folder` if it is still loading (see `fail_catalog_if_loading`).
pub fn fail_catalog_if_loading(agent: &str, folder: &Path, why: &str, cx: &mut App) {
    with_catalogs(cx, |c| {
        c.fail_catalog_if_loading(agent, folder, why);
    });
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

/// Read the folders `provider` itself knows (`Provider::known_folders`, under your home) in the
/// background, once per app run.
pub fn load_known_folders(provider: &'static dyn Provider, cx: &mut App) {
    let agent = provider.info().name;
    let started = with_catalogs(cx, |c| {
        let fresh = c.known_folders(agent).is_none();
        if fresh {
            c.set_known_folders(agent, Lookup::Loading);
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
                crate::workspace::home_dir()
                    .map(|home| provider.known_folders(&home))
                    .unwrap_or_default()
            })
            .await;
        cx.update(|cx| with_catalogs(cx, |c| c.set_known_folders(agent, Lookup::Ready(found))));
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
            used_pct: None,
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
    fn a_catalog_still_loading_fails_when_its_process_ends_unanswered() {
        let mut c = Catalogs::default();
        let a = Path::new("/a");
        // A tab closed (or its process ended) before the answer: the entry doesn't stay loading.
        assert!(c.begin_catalog("claude", a));
        assert!(c.fail_catalog_if_loading("claude", a, "it ended"));
        assert_eq!(
            c.catalog("claude", a),
            Some(&Lookup::Failed("it ended".into()))
        );
        assert!(
            c.begin_catalog("claude", a),
            "so the next asker tries again"
        );
        // A restart: the entry is still loading (begin says no), and the new process ending
        // unanswered fails it too.
        assert!(!c.begin_catalog("claude", a));
        assert!(c.fail_catalog_if_loading("claude", a, "again"));
        // An answer that came is not undone, and an agent never asked is not made up.
        c.set_catalog("claude", a, Lookup::Ready(Catalog::default()));
        assert!(!c.fail_catalog_if_loading("claude", a, "late"));
        assert!(matches!(c.catalog("claude", a), Some(Lookup::Ready(_))));
        assert!(!c.fail_catalog_if_loading("agy", a, "never asked"));
        assert!(c.catalog("agy", a).is_none());
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
    fn a_stale_usage_is_still_the_last_one_read() {
        let mut c = Catalogs::default();
        let t = Instant::now();
        assert!(c.usage_last("claude").is_none());
        c.set_usage("claude", Lookup::Ready(Vec::new()), t);
        assert!(c.usage("claude", t + Duration::from_secs(61)).is_none());
        assert_eq!(c.usage_last("claude"), Some(&Lookup::Ready(Vec::new())));
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

    /// M11aq: a probe stopped (✦'s Esc) is killed at once and says so.
    #[test]
    fn a_stopped_probe_is_killed_and_says_so() {
        let stop = std::sync::atomic::AtomicBool::new(true);
        let started = Instant::now();
        let out = run_probe_stoppable(
            &sh("exec sleep 60"),
            Path::new("/"),
            None,
            Duration::from_secs(30),
            &stop,
        );
        assert_eq!(out, Err("stopped".to_string()));
        assert!(started.elapsed() < Duration::from_secs(3), "at once");
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

    /// M11ap: Claude's live windows (from its stream while it works) over the last `/usage`
    /// reading — a newer live value wins, an older one doesn't; with no reading yet, the live
    /// windows alone.
    #[test]
    fn live_windows_freshen_the_last_reading() {
        use crate::providers::{Reset, UsageLimit};
        let t0 = Instant::now();
        let read = |name: &str, left: u8| UsageLimit::Read {
            name: name.into(),
            left_pct: left,
            resets: None,
        };
        let reading = vec![UsageGroup {
            name: String::new(),
            about: None,
            limits: vec![
                read("Current session", 90),
                read("Current week (Fable)", 40),
            ],
        }];
        let live = |used: u8| RateLimit {
            limit: "Current session".into(),
            state: LimitState::Fine,
            resets_at: Some(1_800_000_000),
            used_pct: Some(used),
        };
        let mut c = Catalogs::default();
        assert_eq!(c.live_usage("claude"), None, "nothing read, nothing live");
        // Live only: its windows.
        c.note_limit_at("claude", live(30), t0);
        let only = c.live_usage("claude").expect("the live window");
        assert_eq!(
            only[0].limits,
            [UsageLimit::Read {
                name: "Current session".into(),
                left_pct: 70,
                resets: Some(Reset::At(1_800_000_000)),
            }]
        );
        // A reading after it: the reading's numbers.
        c.set_usage(
            "claude",
            Lookup::Ready(reading.clone()),
            t0 + Duration::from_secs(5),
        );
        assert_eq!(
            c.live_usage("claude").expect("read")[0].limits,
            reading[0].limits
        );
        // A live value after the reading: it wins for its limit; the others stay.
        c.note_limit_at("claude", live(45), t0 + Duration::from_secs(9));
        let both = c.live_usage("claude").expect("both");
        assert_eq!(
            both[0].limits[0],
            UsageLimit::Read {
                name: "Current session".into(),
                left_pct: 55,
                resets: Some(Reset::At(1_800_000_000)),
            }
        );
        assert_eq!(both[0].limits[1], read("Current week (Fable)", 40));
        // A background read under way (Loading): the last reading still shows, not nothing.
        assert!(c.begin_usage("claude", t0 + USAGE_REFRESH));
        let during = c.live_usage("claude").expect("kept while it reads again");
        assert!(
            during[0].limits.contains(&read("Current week (Fable)", 40)),
            "the reading's own limits too: {during:?}"
        );
    }

    /// M11ap: the background read comes every 5 minutes while the window is seen.
    #[test]
    fn usage_is_read_again_every_five_minutes_while_seen() {
        let t0 = Instant::now();
        assert!(refresh_due(None, t0, true), "never read");
        assert!(!refresh_due(Some(t0), t0 + Duration::from_secs(60), true));
        assert!(refresh_due(Some(t0), t0 + USAGE_REFRESH, true));
        assert!(
            !refresh_due(Some(t0), t0 + USAGE_REFRESH, false),
            "hidden: wait"
        );
    }

    /// The review's finding 4: no background reads while the status bar is off (or the window
    /// unseen).
    #[test]
    fn reads_are_wanted_only_with_the_bar_shown_and_the_window_seen() {
        assert!(reads_wanted(true, true));
        assert!(!reads_wanted(true, false), "the status bar is off");
        assert!(!reads_wanted(false, true), "the window is hidden");
    }

    /// The review's finding 5: a limit whose reset has passed counts as unused, not as the
    /// last number read; kept numbers after a failed read say so.
    #[test]
    fn a_passed_reset_counts_as_unused_and_old_numbers_say_so() {
        use crate::providers::{Reset, UsageLimit};
        let mut c = Catalogs::default();
        let limit = |left, at| UsageLimit::Read {
            name: "Session".into(),
            left_pct: left,
            resets: Some(Reset::At(at)),
        };
        let group = |l| UsageGroup {
            name: String::new(),
            about: None,
            limits: vec![l],
        };
        c.set_usage(
            "claude",
            Lookup::Ready(vec![group(limit(5, 1_000))]),
            Instant::now(),
        );
        let before = c.live_usage_at("claude", 900).expect("read");
        assert_eq!(before[0].limits[0], limit(5, 1_000), "not reset yet");
        let after = c.live_usage_at("claude", 1_001).expect("read");
        assert!(
            matches!(
                after[0].limits[0],
                UsageLimit::Read {
                    left_pct: 100,
                    resets: None,
                    ..
                }
            ),
            "reset since: unused {after:?}"
        );
        assert_eq!(
            stale_note(true, Some(12 * 60)).as_deref(),
            Some("Couldn't refresh · updated 12m ago")
        );
        assert_eq!(
            stale_note(true, Some(10)).as_deref(),
            Some("Couldn't refresh · updated just now")
        );
        assert_eq!(stale_note(false, Some(600)), None, "it refreshed fine");
    }
}
