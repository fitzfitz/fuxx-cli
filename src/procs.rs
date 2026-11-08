//! This Mac's process table (M11i): what runs in a terminal, read from one
//! `ps -A -o pid=,ppid=,pgid=,tpgid=,stat=,args=` run (`/bin/ps`, capped at 2 s, off the UI
//! thread). Zombies (`Z`) never name a program.
//!
//! A shell's row says which process group has its terminal (`tpgid`): its own (or 0) means it
//! is at its prompt; another is the program in the foreground, named by that group's leader
//! (`foreground`). `descendants` gives a shell's whole tree (for the ports lookup).
//!
//! The status bar's ports: the TCP ports a terminal's processes listen on, from one
//! `/usr/sbin/lsof -nP -iTCP -sTCP:LISTEN -a -p <pids> -Fpn` run (capped at 2 s, off the UI
//! thread).
//!
//! What runs in a terminal (`ProgramState`) drives the status bar's word (Running or Ready),
//! its ports (looked up only while a program runs) and an idle row's dot or ring; never a
//! name. `program_name` says which program it is (only to tell one from another) and `Stable`
//! keeps the answer from flickering.
//!
//! `parse_ps`, `foreground`, `descendants`, `parse_lsof` and `read_lsof` are pure and tested on
//! the tools' output as text; `ps_table` and `listening_ports` run them and block, so they are
//! called on the background executor.

use std::collections::{HashMap, HashSet, VecDeque};
use std::process::Command;

use crate::git_branch::{GIT_CAP, Ran, run_capped_bytes, run_capped_outcome};

/// One process as `ps` lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    /// Its process group.
    pub pgid: u32,
    /// The process group in the foreground of its terminal (0: it has no terminal).
    pub tpgid: u32,
    /// Its state (`stat`): `S`, `R+`, `Z` for a zombie…
    pub stat: String,
    /// Its command line, its arguments joined by spaces (a newline in one shows as `\012`).
    pub args: String,
}

impl Proc {
    /// It has ended and waits for its parent to reap it (`Z`).
    pub fn is_zombie(&self) -> bool {
        self.stat.starts_with('Z')
    }
}

/// What runs in a terminal, as far as is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramState {
    /// A program other than the shell is in front: which (`program_name`).
    Running(String),
    /// The shell is at its prompt.
    Prompt,
    /// Not looked up (yet), or it can't be: an SSH or background-service terminal.
    Unknown,
}

/// The coding agents fuxx knows, by the name they run as.
const AGENTS: &[&str] = &[
    "claude", "codex", "opencode", "aider", "gemini", "goose", "amp",
];
/// Agents run from an npm package (`node …/@anthropic-ai/claude-code/cli.js`): the package's
/// path and the agent's name.
const AGENT_PACKAGES: &[(&str, &str)] = &[
    ("@anthropic-ai/claude-code", "claude"),
    ("@openai/codex", "codex"),
    ("@google/gemini-cli", "gemini"),
];

/// Which program a command line from `ps` runs: a known agent (by its name or its package's
/// path, anywhere in it), else the program's file name. Cut at a newline (`ps` writes it
/// `\012`). No argument is kept.
pub fn program_name(args: &str) -> String {
    let line = args.split("\\012").next().unwrap_or_default();
    let line = line.split('\n').next().unwrap_or_default();
    let words: Vec<&str> = line.split_whitespace().collect();
    let agent = words.iter().find_map(|word| {
        let name = file_name(word);
        AGENTS.iter().copied().find(|a| *a == name).or_else(|| {
            AGENT_PACKAGES
                .iter()
                .find(|(package, _)| word.contains(package))
                .map(|(_, agent)| *agent)
        })
    });
    match (agent, words.first()) {
        (Some(agent), _) => agent.to_string(),
        (None, Some(first)) => file_name(first).to_string(),
        (None, None) => String::new(),
    }
}

/// A program's file name: the folder part and a login shell's leading `-` dropped.
fn file_name(word: &str) -> &str {
    let name = word.rsplit('/').next().unwrap_or(word);
    name.trim_start_matches('-')
}

/// Whether `name` (a `program_name`) is one of the coding agents fuxx knows.
pub fn is_known_agent(name: &str) -> bool {
    AGENTS.contains(&name)
}

/// The program a terminal is shown running, kept steady: a new answer is taken only once it
/// comes twice in a row. A known agent is taken at once, and so is the prompt after one; so is
/// the first answer of all (nothing was known before it).
#[derive(Clone, Debug)]
pub struct Stable {
    shown: ProgramState,
    pending: Option<(ProgramState, u8)>,
}

impl Default for Stable {
    fn default() -> Self {
        Self::new()
    }
}

impl Stable {
    /// Nothing known yet.
    pub fn new() -> Stable {
        Stable {
            shown: ProgramState::Unknown,
            pending: None,
        }
    }

    /// A lookup saw `seen`; true when what's shown changed.
    pub fn offer(&mut self, seen: ProgramState) -> bool {
        if seen == self.shown {
            self.pending = None;
            return false;
        }
        let agent = |state: &ProgramState| matches!(state, ProgramState::Running(name) if is_known_agent(name));
        let at_once = self.shown == ProgramState::Unknown
            || agent(&seen)
            || (seen == ProgramState::Prompt && agent(&self.shown));
        let count = match &self.pending {
            Some((pending, count)) if *pending == seen => count + 1,
            _ => 1,
        };
        if at_once || count >= 2 {
            self.shown = seen;
            self.pending = None;
            true
        } else {
            self.pending = Some((seen, count));
            false
        }
    }

    /// What's shown.
    pub fn shown(&self) -> &ProgramState {
        &self.shown
    }
}

/// What a shell's terminal is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Foreground {
    /// The shell has its terminal: it is at its prompt.
    Prompt,
    /// Another job has it: its leader's command line.
    Program(String),
    /// The shell isn't in the table (it has ended, or isn't ours to see).
    Unknown,
}

/// `ps -A -o pid=,ppid=,pgid=,tpgid=,stat=,args=`'s output read: four numbers, the state, then
/// the command line on each line. A line that doesn't read so (short, or not numbers) is skipped.
pub fn parse_ps(stdout: &str) -> Vec<Proc> {
    stdout.lines().filter_map(parse_line).collect()
}

/// One line of `parse_ps`.
fn parse_line(line: &str) -> Option<Proc> {
    let mut rest = line.trim_start();
    let mut number = || -> Option<u32> {
        let end = rest.find(char::is_whitespace)?;
        let n = rest[..end].parse().ok()?;
        rest = rest[end..].trim_start();
        Some(n)
    };
    let (pid, ppid, pgid, tpgid) = (number()?, number()?, number()?, number()?);
    let end = rest.find(char::is_whitespace)?;
    let stat = rest[..end].to_string();
    let args = rest[end..].trim();
    (!args.is_empty()).then(|| Proc {
        pid,
        ppid,
        pgid,
        tpgid,
        stat,
        args: args.to_string(),
    })
}

/// `parse_ps` on `ps`'s raw output, read lossily: one process whose arguments aren't UTF-8
/// mustn't cost the whole table.
pub fn parse_ps_bytes(stdout: &[u8]) -> Vec<Proc> {
    parse_ps(&String::from_utf8_lossy(stdout))
}

/// What the terminal of shell `shell` (its pid) is doing, from `procs`.
pub fn foreground(procs: &[Proc], shell: u32) -> Foreground {
    let Some(me) = procs.iter().find(|p| p.pid == shell) else {
        return Foreground::Unknown;
    };
    let front = me.tpgid;
    if front == 0 || front == me.pgid {
        return Foreground::Prompt;
    }
    // The job's leader names it; gone or a zombie (it ended, not reaped yet), a member still
    // running in it does.
    let live = procs.iter().filter(|p| !p.is_zombie());
    live.clone()
        .find(|p| p.pid == front)
        .or_else(|| live.clone().find(|p| p.pgid == front))
        // The whole job just ended: the shell has its terminal back.
        .map_or(Foreground::Prompt, |p| Foreground::Program(p.args.clone()))
}

/// Every process below `root` (its children, theirs, and so on), `root` not included.
pub fn descendants(procs: &[Proc], root: u32) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in procs {
        children.entry(p.ppid).or_default().push(p.pid);
    }
    let mut seen = HashSet::from([root]);
    let mut queue = VecDeque::from([root]);
    let mut below = Vec::new();
    while let Some(pid) = queue.pop_front() {
        for &child in children.get(&pid).into_iter().flatten() {
            // `seen`: a table read mid-change can't loop.
            if seen.insert(child) {
                below.push(child);
                queue.push_back(child);
            }
        }
    }
    below
}

/// This Mac's processes, now; none when `ps` fails or takes longer than `GIT_CAP` (2 s).
/// Blocks: call it off the UI thread.
pub fn ps_table() -> Option<Vec<Proc>> {
    let out = run_capped_bytes(ps_command(), None, GIT_CAP).success()?;
    Some(parse_ps_bytes(&out))
}

/// The `ps` run: in a UTF-8 locale, since in the C locale (an app opened from the Finder) it
/// writes non-ASCII as `M-CM-)`.
fn ps_command() -> Command {
    let mut ps = Command::new("/bin/ps");
    ps.args(["-A", "-o", "pid=,ppid=,pgid=,tpgid=,stat=,args="])
        .env("LC_ALL", "en_US.UTF-8");
    ps
}

/// `lsof -Fpn`'s output read: the port of each `n` line (`n*:3000`, `n127.0.0.1:5173`,
/// `n[::1]:8080`), sorted, each once (a server on IPv4 and IPv6 is one port). The `p` and `f`
/// lines, and any line that doesn't read so, are skipped.
pub fn parse_lsof(stdout: &str) -> Vec<u16> {
    let mut ports: Vec<u16> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix('n'))
        // A connection (`local->remote`) isn't a listening socket.
        .filter(|name| !name.contains("->"))
        .filter_map(|name| name.rsplit_once(':')?.1.parse().ok())
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// What an `lsof` run said, read: its ports when it ran to its end (1 too: lsof says 1 when one
/// of the pids has ended or none listens, yet lists the rest); none when it failed, wasn't
/// there, or ran past the cap.
pub(crate) fn read_lsof(ran: Ran) -> Option<Vec<u16>> {
    match ran {
        Ran::Exited(Some(0 | 1), out) => Some(parse_lsof(&out)),
        _ => None,
    }
}

/// The TCP ports processes `pids` listen on, now; none when `lsof` fails or takes longer than
/// `GIT_CAP` (2 s). No pids: none listen, and nothing is run. Blocks: call it off the UI
/// thread.
pub fn listening_ports(pids: &[u32]) -> Option<Vec<u16>> {
    let Some(lsof) = lsof_command(pids) else {
        return Some(Vec::new());
    };
    read_lsof(run_capped_outcome(lsof, None, GIT_CAP))
}

/// The `lsof` run for `pids`: listening TCP sockets (`-a`: and) of those processes, numbers
/// left as numbers (`-nP`), one field per line (`-Fpn`); none for no pids (nothing to run).
fn lsof_command(pids: &[u32]) -> Option<Command> {
    if pids.is_empty() {
        return None;
    }
    let list: Vec<String> = pids.iter().map(u32::to_string).collect();
    let mut lsof = Command::new("/usr/sbin/lsof");
    lsof.args(["-nP", "-iTCP", "-sTCP:LISTEN", "-a", "-p"])
        .arg(list.join(","))
        .arg("-Fpn");
    Some(lsof)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &str) -> ProgramState {
        ProgramState::Running(name.into())
    }

    #[test]
    fn programs_are_known_by_an_agent_or_their_file_name() {
        let cases = [
            ("claude", "claude"),
            (
                "node /opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js",
                "claude",
            ),
            (
                "node /usr/local/lib/node_modules/@openai/codex/bin/codex.js",
                "codex",
            ),
            ("node /x/@google/gemini-cli/dist/index.js", "gemini"),
            ("/opt/homebrew/bin/aider --model x", "aider"),
            ("codex --full-auto", "codex"),
            ("node /usr/local/bin/npm run dev", "node"),
            ("cargo test --release", "cargo"),
            ("/usr/bin/vim src/main.rs", "vim"),
            ("python3 manage.py runserver", "python3"),
            ("-zsh", "zsh"),
            ("/bin/zsh -l", "zsh"),
            ("foo\\012bar", "foo"),
            ("", ""),
        ];
        for (args, name) in cases {
            assert_eq!(program_name(args), name, "{args}");
        }
    }

    #[test]
    fn no_argument_is_kept() {
        for (args, hidden) in [
            ("cargo run -- --token abc123", "abc123"),
            ("docker run -e API_KEY=sk-live-0123456789 img", "sk-live"),
            ("git -C /Users/me/repo status", "/Users"),
        ] {
            let name = program_name(args);
            assert!(!name.contains(hidden), "{args} keeps {hidden}");
            assert!(!name.contains(' '), "{args}: one word ({name})");
        }
    }

    #[test]
    fn the_known_agents_are_known() {
        for agent in [
            "claude", "codex", "opencode", "aider", "gemini", "goose", "amp",
        ] {
            assert!(is_known_agent(agent), "{agent}");
        }
        for other in ["make", "vim", "npm", "zsh", ""] {
            assert!(!is_known_agent(other), "{other}");
        }
    }

    #[test]
    fn a_new_program_shows_after_two_answers_in_a_row() {
        let mut s = Stable::new();
        assert!(s.offer(ProgramState::Prompt), "the first answer is taken");
        assert!(!s.offer(run("make")));
        assert_eq!(s.shown(), &ProgramState::Prompt);
        assert!(s.offer(run("make")));
        assert_eq!(s.shown(), &run("make"));
        // The same answer again changes nothing.
        assert!(!s.offer(run("make")));
    }

    #[test]
    fn a_known_agent_and_the_prompt_after_it_show_at_once() {
        let mut s = Stable::new();
        s.offer(ProgramState::Prompt);
        assert!(s.offer(run("claude")));
        assert_eq!(s.shown(), &run("claude"));
        assert!(s.offer(ProgramState::Prompt));
        assert_eq!(s.shown(), &ProgramState::Prompt);
    }

    #[test]
    fn alternating_answers_never_change_what_is_shown() {
        let mut s = Stable::new();
        s.offer(ProgramState::Prompt);
        for _ in 0..4 {
            assert!(!s.offer(run("ls")));
            assert!(!s.offer(run("grep")));
        }
        assert_eq!(s.shown(), &ProgramState::Prompt);
        // Nor does a one-off answer between two of the same.
        assert!(!s.offer(run("make")));
        assert!(!s.offer(ProgramState::Prompt));
        assert!(!s.offer(run("make")));
        assert_eq!(s.shown(), &ProgramState::Prompt);
    }

    fn proc(pid: u32, ppid: u32, pgid: u32, tpgid: u32, stat: &str, args: &str) -> Proc {
        Proc {
            pid,
            ppid,
            pgid,
            tpgid,
            stat: stat.into(),
            args: args.into(),
        }
    }

    fn p(pid: u32, ppid: u32, pgid: u32, tpgid: u32, args: &str) -> Proc {
        proc(pid, ppid, pgid, tpgid, "S", args)
    }

    /// A zombie: it has ended, its parent hasn't reaped it.
    fn zombie(pid: u32, ppid: u32, pgid: u32, tpgid: u32, args: &str) -> Proc {
        proc(pid, ppid, pgid, tpgid, "Z+", args)
    }

    #[test]
    fn ps_lines_read_as_four_numbers_the_state_and_the_command_line() {
        // As `ps` printed them on this Mac (right-aligned numbers).
        let out = "39856 39855 39856 39882 Ss -/bin/zsh\n39882 39856 39882 39882 S+ claude\n";
        assert_eq!(
            parse_ps(out),
            [
                proc(39856, 39855, 39856, 39882, "Ss", "-/bin/zsh"),
                proc(39882, 39856, 39882, 39882, "S+", "claude"),
            ]
        );
        let padded = "  501     1   501     0 S    /usr/bin/vim src/main.rs  \n";
        assert_eq!(
            parse_ps(padded),
            [p(501, 1, 501, 0, "/usr/bin/vim src/main.rs")]
        );
        // A newline in an argument is `\012`; it stays as written.
        let newline = "7 1 7 7 S sh -c echo\\012hi\n";
        assert_eq!(parse_ps(newline)[0].args, "sh -c echo\\012hi");
    }

    #[test]
    fn lines_that_dont_read_are_skipped() {
        let out = "12 1 12\nabc 1 2 3 S zsh\n\n40 1 40 40 S vim\n13 1 13 13 S\n14 1 14 14\n";
        assert_eq!(parse_ps(out), [p(40, 1, 40, 40, "vim")]);
        assert_eq!(parse_ps(""), []);
    }

    #[test]
    fn output_that_isn_t_utf_8_still_reads_every_line() {
        // One process's argv isn't UTF-8 (Latin-1 é): its line reads lossily, the rest as is.
        let out = b"40 1 40 40 S vim caf\xe9.txt\n41 1 41 41 S+ claude\n";
        let procs = parse_ps_bytes(out);
        assert_eq!(procs.len(), 2);
        assert_eq!(procs[0].args, "vim caf\u{fffd}.txt");
        assert_eq!(procs[1], proc(41, 1, 41, 41, "S+", "claude"));
    }

    #[test]
    fn ps_asks_for_the_state_column_and_utf_8() {
        let ps = ps_command();
        assert_eq!(ps.get_program(), "/bin/ps");
        let args: Vec<_> = ps.get_args().collect();
        assert_eq!(args, ["-A", "-o", "pid=,ppid=,pgid=,tpgid=,stat=,args="]);
        // In the C locale (an app opened from the Finder) ps writes `é` as `M-CM-)`.
        let lc_all = ps
            .get_envs()
            .find(|(k, _)| *k == "LC_ALL")
            .and_then(|(_, v)| v);
        assert_eq!(lc_all, Some(std::ffi::OsStr::new("en_US.UTF-8")));
    }

    #[test]
    fn a_zombie_never_names_the_program() {
        // The leader has ended (not reaped yet); a member still runs: the member names it.
        let procs = [
            p(100, 1, 100, 200, "-zsh"),
            zombie(200, 100, 200, 200, "npm run dev"),
            p(201, 1, 200, 200, "node server.js"),
        ];
        assert_eq!(
            foreground(&procs, 100),
            Foreground::Program("node server.js".into())
        );
        // Only zombies left in the job: the shell has its terminal back.
        let procs = [
            p(100, 1, 100, 200, "-zsh"),
            zombie(200, 100, 200, 200, "make"),
        ];
        assert_eq!(foreground(&procs, 100), Foreground::Prompt);
    }

    #[test]
    fn the_shell_s_own_group_in_front_is_its_prompt() {
        let procs = [p(100, 1, 100, 100, "-zsh")];
        assert_eq!(foreground(&procs, 100), Foreground::Prompt);
        // No terminal in front at all.
        let procs = [p(100, 1, 100, 0, "-zsh")];
        assert_eq!(foreground(&procs, 100), Foreground::Prompt);
    }

    #[test]
    fn another_group_in_front_is_the_program_its_leader_runs() {
        let procs = [
            p(100, 1, 100, 200, "-zsh"),
            p(200, 100, 200, 200, "npm run dev"),
            p(201, 200, 200, 200, "node server.js"),
        ];
        assert_eq!(
            foreground(&procs, 100),
            Foreground::Program("npm run dev".into())
        );
        // The leader is gone but its group isn't: a member names it.
        let procs = [
            p(100, 1, 100, 200, "-zsh"),
            p(201, 1, 200, 200, "node server.js"),
        ];
        assert_eq!(
            foreground(&procs, 100),
            Foreground::Program("node server.js".into())
        );
    }

    #[test]
    fn a_shell_not_in_the_table_is_unknown() {
        let procs = [p(100, 1, 100, 100, "-zsh")];
        assert_eq!(foreground(&procs, 999), Foreground::Unknown);
        assert_eq!(foreground(&[], 100), Foreground::Unknown);
    }

    #[test]
    fn descendants_are_the_whole_tree_below_without_the_root() {
        let procs = [
            p(1, 0, 1, 0, "launchd"),
            p(100, 1, 100, 200, "-zsh"),
            p(200, 100, 200, 200, "npm run dev"),
            p(300, 200, 200, 200, "node server.js"),
            p(400, 300, 200, 200, "esbuild"),
            // A sibling of the shell and its child: not in its tree.
            p(500, 1, 500, 500, "-zsh"),
            p(600, 500, 600, 500, "vim"),
        ];
        let mut below = descendants(&procs, 100);
        below.sort();
        assert_eq!(below, [200, 300, 400]);
        assert_eq!(descendants(&procs, 400), Vec::<u32>::new());
        assert_eq!(descendants(&procs, 999), Vec::<u32>::new());
    }

    #[test]
    fn lsof_ports_merge_ipv4_ipv6_and_any_and_come_sorted() {
        // As `lsof -Fpn` prints them: a `p` line per process, `f` and `n` lines per socket.
        let out = "p501\nf3\nn*:3000\nf4\nn[::1]:3000\nf5\nn127.0.0.1:3000\n\
                   p502\nf7\nn[::]:8080\nf8\nn127.0.0.1:5173\n";
        assert_eq!(parse_lsof(out), [3000, 5173, 8080]);
    }

    #[test]
    fn lsof_lines_that_dont_name_a_port_are_skipped() {
        // A process with no socket listed: its `p` line alone.
        assert_eq!(parse_lsof("p501\n"), Vec::<u16>::new());
        let out =
            "garbage\nn*:abc\nn*:99999\nnlocalhost\nn*:\n\nn10.0.0.1:22->10.0.0.2:5000\nn*:4000\n";
        assert_eq!(parse_lsof(out), [4000]);
        assert_eq!(parse_lsof(""), Vec::<u16>::new());
    }

    #[test]
    fn an_lsof_that_ran_to_its_end_is_read_and_anything_else_is_none() {
        let listed = "p501\nf3\nn*:3000\n".to_string();
        assert_eq!(
            read_lsof(Ran::Exited(Some(0), listed.clone())),
            Some(vec![3000])
        );
        // 1: a pid had ended (or nothing listened); what it found still counts.
        assert_eq!(read_lsof(Ran::Exited(Some(1), listed)), Some(vec![3000]));
        assert_eq!(read_lsof(Ran::Exited(Some(1), String::new())), Some(vec![]));
        assert_eq!(read_lsof(Ran::Exited(Some(2), String::new())), None);
        assert_eq!(read_lsof(Ran::Exited(None, String::new())), None);
        assert_eq!(read_lsof(Ran::TimedOut), None);
        // Missing (not on this Mac) or unreadable.
        assert_eq!(read_lsof(Ran::Failed), None);
    }

    #[test]
    fn no_pids_listen_on_nothing_and_run_nothing() {
        // No lsof run for no pids...
        assert!(lsof_command(&[]).is_none());
        // ...and Some, not None (a failure).
        assert_eq!(listening_ports(&[]), Some(vec![]));
    }

    #[test]
    fn lsof_asks_for_the_pids_listening_tcp_sockets_by_absolute_path() {
        let lsof = lsof_command(&[200, 300, 400]).expect("pids: a run");
        assert_eq!(lsof.get_program(), "/usr/sbin/lsof");
        let args: Vec<_> = lsof.get_args().collect();
        assert_eq!(
            args,
            [
                "-nP",
                "-iTCP",
                "-sTCP:LISTEN",
                "-a",
                "-p",
                "200,300,400",
                "-Fpn"
            ]
        );
    }
}
