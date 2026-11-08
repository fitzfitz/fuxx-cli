//! This Mac's process table (M11i): what runs in a terminal, read from one
//! `ps -A -o pid=,ppid=,pgid=,tpgid=,stat=,args=` run (`/bin/ps`, capped at 2 s, off the UI
//! thread). Zombies (`Z`) never name a program.
//!
//! A shell's row says which process group has its terminal (`tpgid`): its own (or 0) means it
//! is at its prompt; another is the program in the foreground, named by that group's leader
//! (`foreground`). `descendants` gives a shell's whole tree (for the ports lookup).
//!
//! `parse_ps`, `foreground` and `descendants` are pure and tested on `ps` output as text;
//! `ps_table` runs `ps` and blocks, so it is called on the background executor.

use std::collections::{HashMap, HashSet, VecDeque};
use std::process::Command;

use crate::git_branch::{GIT_CAP, run_capped_bytes};

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
#[allow(dead_code)] // the status bar's ports lookup uses it (M11i Task 7)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
