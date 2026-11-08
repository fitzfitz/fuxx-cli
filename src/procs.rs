//! This Mac's process table (M11i): what runs in a terminal, read from one
//! `ps -A -o pid=,ppid=,pgid=,tpgid=,args=` run (`/bin/ps`, capped at 2 s, off the UI thread).
//!
//! A shell's row says which process group has its terminal (`tpgid`): its own (or 0) means it
//! is at its prompt; another is the program in the foreground, named by that group's leader
//! (`foreground`). `descendants` gives a shell's whole tree (for the ports lookup).
//!
//! `parse_ps`, `foreground` and `descendants` are pure and tested on `ps` output as text;
//! `ps_table` runs `ps` and blocks, so it is called on the background executor.

use std::collections::{HashMap, HashSet, VecDeque};
use std::process::Command;

use crate::git_branch::{GIT_CAP, run_capped_outcome};

/// One process as `ps` lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    /// Its process group.
    pub pgid: u32,
    /// The process group in the foreground of its terminal (0: it has no terminal).
    pub tpgid: u32,
    /// Its command line, its arguments joined by spaces (a newline in one shows as `\012`).
    pub args: String,
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

/// `ps -A -o pid=,ppid=,pgid=,tpgid=,args=`'s output read: four numbers then the command line
/// on each line. A line that doesn't read so (short, or not numbers) is skipped.
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
    let args = rest.trim_end();
    (!args.is_empty()).then(|| Proc {
        pid,
        ppid,
        pgid,
        tpgid,
        args: args.to_string(),
    })
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
    // The job's leader names it; gone (it handed off and ended), a member still in it does.
    procs
        .iter()
        .find(|p| p.pid == front)
        .or_else(|| procs.iter().find(|p| p.pgid == front))
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
    let mut ps = Command::new("/bin/ps");
    ps.args(["-A", "-o", "pid=,ppid=,pgid=,tpgid=,args="]);
    let out = run_capped_outcome(ps, None, GIT_CAP).success()?;
    Some(parse_ps(&out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: u32, pgid: u32, tpgid: u32, args: &str) -> Proc {
        Proc {
            pid,
            ppid,
            pgid,
            tpgid,
            args: args.into(),
        }
    }

    #[test]
    fn ps_lines_read_as_four_numbers_and_the_command_line() {
        // As `ps` printed them on this Mac (right-aligned numbers).
        let out = "39856 39855 39856 39882 -/bin/zsh\n39882 39856 39882 39882 claude\n";
        assert_eq!(
            parse_ps(out),
            [
                p(39856, 39855, 39856, 39882, "-/bin/zsh"),
                p(39882, 39856, 39882, 39882, "claude"),
            ]
        );
        let padded = "  501     1   501     0 /usr/bin/vim src/main.rs  \n";
        assert_eq!(
            parse_ps(padded),
            [p(501, 1, 501, 0, "/usr/bin/vim src/main.rs")]
        );
        // A newline in an argument is `\012`; it stays as written.
        let newline = "7 1 7 7 sh -c echo\\012hi\n";
        assert_eq!(parse_ps(newline)[0].args, "sh -c echo\\012hi");
    }

    #[test]
    fn lines_that_dont_read_are_skipped() {
        let out = "12 1 12\nabc 1 2 3 zsh\n\n40 1 40 40 vim\n13 1 13 13\n";
        assert_eq!(parse_ps(out), [p(40, 1, 40, 40, "vim")]);
        assert_eq!(parse_ps(""), []);
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
