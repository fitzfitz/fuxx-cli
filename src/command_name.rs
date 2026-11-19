//! What leaves the terminal of a command line: its short form, and nothing more.
//!
//! A command line may hold secrets (`API_KEY=… npm run deploy --token …`, `echo sk-…`), so only the
//! program, a plain subcommand after it when the program is one known to take subcommands
//! (`SUBCOMMANDS`), and an ellipsis when another command follows may be shown anywhere: a pane's
//! name, a banner, the 🔔 list, Activity. Mark text is untrusted, so the result is cleaned and
//! capped here.

use std::time::{Duration, Instant};

use crate::agent_detect::{is_assignment, unquote};

/// The longest program name kept, in characters.
pub const PROGRAM_CAP: usize = 32;
/// The longest subcommand word kept, in characters.
pub const WORD_CAP: usize = 24;
/// What a banner calls a command that has no short form.
pub const NO_SHORT: &str = "command";

/// Programs that run the next command: they and their options are skipped.
const WRAPPERS: [&str; 6] = ["sudo", "env", "time", "nohup", "command", "exec"];
/// Programs whose next plain word is a subcommand (`git push`, `pnpm dev`), so it may be shown.
/// Any other program shows its name alone: its next word may be a value, even a secret
/// (`echo sk-…`, `./deploy.sh prod`). The user's call, 2026-11-20.
const SUBCOMMANDS: [&str; 34] = [
    "npm",
    "npx",
    "pnpm",
    "pnpx",
    "yarn",
    "bun",
    "bunx",
    "deno",
    "cargo",
    "rustup",
    "git",
    "gh",
    "go",
    "make",
    "just",
    "docker",
    "podman",
    "kubectl",
    "helm",
    "terraform",
    "brew",
    "pip",
    "pip3",
    "uv",
    "poetry",
    "mix",
    "dotnet",
    "mvn",
    "gradle",
    "gradlew",
    "flutter",
    "dart",
    "swift",
    "bundle",
];
/// Words after which one more word is the thing run (`npm run build`).
const RUNNERS: [&str; 4] = ["run", "exec", "x", "dlx"];

/// The first command's raw words (quotes kept), whether another command follows it, and where
/// the next command starts when the first ends at `&&`, `;` or a new line (not a pipe). `None`
/// for an unclosed quote anywhere in the line.
fn first_command(line: &str) -> Option<(Vec<String>, bool, Option<usize>)> {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut stopped = false;
    let mut quiet = false; // stopped by a lone `&`: what follows is not "another command"
    let mut more = false;
    let mut rest = None;
    let mut chars = line.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            word.push(c);
            continue;
        }
        if c == '\\' || c == '\'' || c == '"' {
            word.push(c);
            if c == '\\' {
                word.extend(chars.next().map(|(_, c)| c));
            } else {
                quote = Some(c);
            }
            continue;
        }
        if stopped {
            // Only the quotes of the rest matter, and whether any of it is not blank.
            more |= !c.is_whitespace();
            continue;
        }
        let stops =
            matches!(c, ';' | '|' | '\n') || (c == '&' && chars.peek().map(|p| p.1) == Some('&'));
        if !(stops || c.is_whitespace()) {
            word.push(c);
            continue;
        }
        if word == "&" {
            quiet = true;
            stopped = true;
        } else if !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        if stops {
            stopped = true;
            rest = match c {
                '&' => chars.next().map(|(i, _)| i + 1),
                '|' => None,
                _ => Some(at + 1),
            };
        }
    }
    if quote.is_some() {
        return None;
    }
    if !stopped && !word.is_empty() && word != "&" {
        words.push(word);
    }
    Some((words, more && !quiet, rest))
}

/// A word of ASCII letters, digits and `-_.:`, 1 to `WORD_CAP` characters, not starting with `-`.
fn plain(word: &str) -> bool {
    (1..=WORD_CAP).contains(&word.chars().count())
        && !word.starts_with('-')
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

/// The short form of a command line: the program, a plain subcommand after it when the program is
/// in `SUBCOMMANDS` (and after `run`, `exec`, `x` or `dlx` one more), and ` …` when another command
/// follows. A leading `cd <folder> &&` (or `;`) is skipped: the command after it is named. `None`
/// when no program can be named.
pub fn short_form(line: &str) -> Option<String> {
    let (words, more, rest) = first_command(line)?;
    if words.first().is_some_and(|w| unquote(w) == "cd")
        && words.len() <= 2
        && let Some(after) = rest.and_then(|at| short_form(&line[at..]))
    {
        return Some(after);
    }
    let mut i = 0;
    while words.get(i).is_some_and(|w| is_assignment(w)) {
        i += 1;
    }
    let mut last_wrapper = None;
    while let Some(w) = words.get(i).filter(|w| WRAPPERS.contains(&unquote(w))) {
        last_wrapper = Some(w);
        i += 1;
        while words
            .get(i)
            .is_some_and(|w| w.starts_with('-') || is_assignment(w))
        {
            i += 1;
        }
    }
    let program = match words.get(i) {
        Some(w) => {
            i += 1;
            w
        }
        None => last_wrapper?,
    };
    let program = unquote(program);
    if program.is_empty() || program.starts_with(['$', '(', '`']) {
        return None;
    }
    let name: String = program
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(PROGRAM_CAP)
        .collect();
    if name.is_empty() {
        return None;
    }
    let takes_subcommand = SUBCOMMANDS.contains(&name.as_str());
    let mut out = name;
    if let Some(w) = words.get(i).filter(|w| takes_subcommand && plain(w)) {
        out.push(' ');
        out.push_str(w);
        if RUNNERS.contains(&w.as_str())
            && let Some(w2) = words.get(i + 1).filter(|w| plain(w))
        {
            out.push(' ');
            out.push_str(w2);
        }
    }
    if more {
        out.push_str(" …");
    }
    Some(out)
}

/// How a command ended, from the exit code its end mark carried (if any).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Done,
    Stopped,
    Failed(i32),
    Finished,
}

impl Ending {
    /// 0 is done, 130 (⌃C) stopped, any other code failed; no code just finished.
    pub fn of(exit: Option<i32>) -> Ending {
        match exit {
            Some(0) => Ending::Done,
            Some(130) => Ending::Stopped,
            Some(n) => Ending::Failed(n),
            None => Ending::Finished,
        }
    }

    pub fn failed(self) -> bool {
        matches!(self, Ending::Failed(_))
    }
}

/// A run's length in its two largest units, the smaller left out when zero: `40s`, `3m 12s`,
/// `2m`, `1h 4m`, `1d 2h`.
pub fn ran_label(d: Duration) -> String {
    let s = d.as_secs();
    let units = [(86_400, 'd'), (3600, 'h'), (60, 'm'), (1, 's')];
    let Some(i) = units.iter().position(|&(n, _)| s >= n) else {
        return "0s".to_string();
    };
    let (big, big_c) = units[i];
    let mut out = format!("{}{big_c}", s / big);
    if let Some(&(small, small_c)) = units.get(i + 1) {
        let rest = (s % big) / small;
        if rest > 0 {
            out.push_str(&format!(" {rest}{small_c}"));
        }
    }
    out
}

/// The banner's body for an ending.
pub fn ending_words(ending: Ending, ran: Duration) -> String {
    let d = ran_label(ran);
    match ending {
        Ending::Done => format!("Done in {d}"),
        Ending::Stopped => format!("Stopped after {d}"),
        Ending::Failed(n) => format!("Failed (exit {n}) after {d}"),
        Ending::Finished => format!("Finished after {d}"),
    }
}

/// A command that ended: its short form (if it had one), how, and how long it ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    pub short: Option<String>,
    pub ending: Ending,
    pub ran: Duration,
}

impl Finished {
    pub fn name(&self) -> &str {
        self.short.as_deref().unwrap_or(NO_SHORT)
    }

    pub fn words(&self) -> String {
        ending_words(self.ending, self.ran)
    }

    /// The 🔔 list's line.
    pub fn listed(&self) -> String {
        format!("{} · {}", self.name(), self.words())
    }

    /// Activity's line.
    pub fn logged(&self) -> String {
        format!("{} — {}", self.name(), self.words())
    }
}

/// What the OSC 133 marks say about the command at the shell prompt, already reduced to its
/// short form: the full command line never leaves the parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandMark {
    /// A command started; `short` is its short form, if it had a command line to make one from.
    Started { short: Option<String> },
    /// The command ended; `exit` is its status when the shell reported one.
    Ended { exit: Option<i32> },
    /// A new prompt: nothing runs.
    Prompt,
    /// What runs is not known any more (a reattach snapshot); never made by `blocks::advance`.
    Unknown,
}

/// What a mark changed in what runs now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Started(Option<String>),
    Ended(Finished),
    Cleared,
}

/// The command running at a shell prompt now, and when it started.
#[derive(Default)]
pub struct CommandRun {
    running: Option<(Option<String>, Instant)>,
}

impl CommandRun {
    pub fn on_mark(&mut self, mark: CommandMark, now: Instant) -> Option<Change> {
        match mark {
            CommandMark::Started { short } => {
                self.running = Some((short.clone(), now));
                Some(Change::Started(short))
            }
            CommandMark::Ended { exit } => {
                let (short, since) = self.running.take()?;
                Some(Change::Ended(Finished {
                    short,
                    ending: Ending::of(exit),
                    ran: now.saturating_duration_since(since),
                }))
            }
            CommandMark::Prompt | CommandMark::Unknown => {
                self.running.take().map(|_| Change::Cleared)
            }
        }
    }

    /// The running command's short form, if it has one.
    pub fn running(&self) -> Option<&str> {
        self.running.as_ref()?.0.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(rows: &[(&str, Option<&str>)]) {
        for (line, want) in rows {
            assert_eq!(short_form(line).as_deref(), *want, "line: {line:?}");
        }
    }

    #[test]
    fn the_specs_table() {
        check(&[
            ("pnpm dev", Some("pnpm dev")),
            ("cargo test --release -- --nocapture", Some("cargo test")),
            (
                "API_KEY=sk-123 npm run deploy --token abc",
                Some("npm run deploy"),
            ),
            ("sudo -E ./scripts/build.sh prod", Some("build.sh")),
            ("cargo build && cargo test", Some("cargo build …")),
            (
                "curl -H \"Authorization: Bearer x\" https://…",
                Some("curl"),
            ),
            ("$(which node) server.js", None),
        ]);
    }

    #[test]
    fn the_on_screen_checks_commands() {
        check(&[
            ("sleep 5", Some("sleep")),
            ("sleep 8; true", Some("sleep …")),
            ("sh -c 'sleep 4; exit 3'", Some("sh")),
            ("FOO=secret sleep 6 --x", Some("sleep")),
            ("npm run build", Some("npm run build")),
            ("vim", Some("vim")),
            ("claude", Some("claude")),
        ]);
    }

    #[test]
    fn wrappers_and_their_options_are_skipped() {
        check(&[
            ("env -i PATH=/bin make test", Some("make test")),
            ("time cargo build", Some("cargo build")),
            ("nohup ./run.sh &", Some("run.sh")),
            ("command git status", Some("git status")),
            ("exec zsh", Some("zsh")),
            ("sudo -i", Some("sudo")),
            ("sudo env FOO=1 nohup make", Some("make")),
        ]);
    }

    #[test]
    fn run_exec_x_and_dlx_take_one_more_word() {
        check(&[
            ("pnpm dlx cowsay hi", Some("pnpm dlx cowsay")),
            ("bun x tsc --noEmit", Some("bun x tsc")),
            ("docker exec -it web sh", Some("docker exec")),
            ("npm run", Some("npm run")),
        ]);
    }

    #[test]
    fn only_a_plain_word_follows_the_program() {
        let long = format!("git {}", "a".repeat(25));
        check(&[
            ("export TOKEN=abc", Some("export")),
            ("mysql -psecret", Some("mysql")),
            ("echo \"a && b\"", Some("echo")),
            (&long, Some("git")),
            ("git é", Some("git")),
        ]);
    }

    #[test]
    fn another_command_adds_an_ellipsis() {
        check(&[
            ("make;ls", Some("make …")),
            ("a|b", Some("a …")),
            ("ls 2>&1 | less", Some("ls …")),
            ("make\nls", Some("make …")),
            ("cargo build &&", Some("cargo build")),
            ("sleep 5 &", Some("sleep")),
        ]);
    }

    #[test]
    fn only_programs_that_take_subcommands_keep_a_second_word() {
        check(&[
            ("echo sk-live-123", Some("echo")),
            ("printf hunter2", Some("printf")),
            ("mytool s3cret", Some("mytool")),
            ("./deploy.sh prod", Some("deploy.sh")),
            ("ssh admin.example", Some("ssh")),
            ("git push", Some("git push")),
            ("yarn build", Some("yarn build")),
            ("go test", Some("go test")),
            ("docker compose", Some("docker compose")),
            ("kubectl apply", Some("kubectl apply")),
            ("just lint", Some("just lint")),
        ]);
    }

    #[test]
    fn a_leading_cd_is_skipped() {
        check(&[
            ("cd web && pnpm dev", Some("pnpm dev")),
            ("cd web; pnpm dev", Some("pnpm dev")),
            ("cd \"my app\" && cargo test", Some("cargo test")),
            ("cd a && cd b && make test && make", Some("make test …")),
            ("cd web", Some("cd")),
            ("cd web &&", Some("cd")),
            ("cd web | less", Some("cd …")),
        ]);
    }

    #[test]
    fn lines_that_cannot_be_read_have_none() {
        check(&[
            ("", None),
            ("   ", None),
            ("(cd src; make)", None),
            ("`which node` x", None),
            ("$EDITOR notes", None),
            ("echo \"unclosed", None),
            ("FOO=1", None),
        ]);
    }

    #[test]
    fn control_characters_and_long_names_are_cut() {
        let long = "a".repeat(40);
        assert_eq!(short_form(&long), Some("a".repeat(PROGRAM_CAP)));
        assert_eq!(
            short_form("ma\u{7}ke\u{1b} test").as_deref(),
            Some("make test")
        );
        for line in [&long[..], "ma\u{7}ke\u{1b}[31m x"] {
            let got = short_form(line).unwrap();
            assert!(!got.chars().any(|c| c.is_control()), "{got:?}");
        }
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn the_banner_tables_bodies() {
        let rows = [
            (Some(0), 192, "Done in 3m 12s"),
            (Some(130), 3840, "Stopped after 1h 4m"),
            (Some(101), 40, "Failed (exit 101) after 40s"),
            (None, 120, "Finished after 2m"),
            (Some(-1), 5, "Failed (exit -1) after 5s"),
            (Some(256), 5, "Failed (exit 256) after 5s"),
        ];
        for (exit, s, want) in rows {
            assert_eq!(ending_words(Ending::of(exit), secs(s)), want);
        }
    }

    #[test]
    fn endings_from_the_exit_code() {
        assert_eq!(Ending::of(Some(0)), Ending::Done);
        assert_eq!(Ending::of(Some(130)), Ending::Stopped);
        assert!(!Ending::Stopped.failed());
        assert!(!Ending::Done.failed());
        assert!(!Ending::Finished.failed());
        for n in [1, 101, 143, 255, 256, -1] {
            assert_eq!(Ending::of(Some(n)), Ending::Failed(n));
            assert!(Ending::of(Some(n)).failed());
        }
        assert_eq!(Ending::of(None), Ending::Finished);
    }

    #[test]
    fn durations_use_two_units() {
        let rows = [
            (0, "0s"),
            (8, "8s"),
            (59, "59s"),
            (60, "1m"),
            (192, "3m 12s"),
            (3600, "1h"),
            (3661, "1h 1m"),
            (3840, "1h 4m"),
            (86_400, "1d"),
            (93_600, "1d 2h"),
            (90_300, "1d 1h"),
            (691_200, "8d"),
        ];
        for (s, want) in rows {
            assert_eq!(ran_label(secs(s)), want, "{s} s");
        }
    }

    #[test]
    fn a_finished_command_in_the_list_and_in_activity() {
        let f = Finished {
            short: Some("pnpm build".to_string()),
            ending: Ending::Failed(101),
            ran: secs(40),
        };
        assert_eq!(f.listed(), "pnpm build · Failed (exit 101) after 40s");
        assert_eq!(f.logged(), "pnpm build — Failed (exit 101) after 40s");
        let none = Finished {
            short: None,
            ending: Ending::Done,
            ran: secs(3),
        };
        assert_eq!(none.name(), "command");
        assert_eq!(none.listed(), "command · Done in 3s");
    }

    fn started(short: &str) -> CommandMark {
        CommandMark::Started {
            short: Some(short.to_string()),
        }
    }

    #[test]
    fn a_command_runs_from_its_start_to_its_end() {
        let mut run = CommandRun::default();
        let t0 = Instant::now();
        assert_eq!(
            run.on_mark(started("pnpm dev"), t0),
            Some(Change::Started(Some("pnpm dev".to_string())))
        );
        assert_eq!(run.running(), Some("pnpm dev"));
        let end = CommandMark::Ended { exit: Some(0) };
        assert_eq!(
            run.on_mark(end, t0 + Duration::from_secs(192)),
            Some(Change::Ended(Finished {
                short: Some("pnpm dev".to_string()),
                ending: Ending::Done,
                ran: Duration::from_secs(192),
            }))
        );
        assert_eq!(run.running(), None);
    }

    #[test]
    fn an_end_with_no_start_is_nothing() {
        let mut run = CommandRun::default();
        let end = CommandMark::Ended { exit: Some(0) };
        assert_eq!(run.on_mark(end, Instant::now()), None);
        assert_eq!(run.running(), None);
    }

    #[test]
    fn a_prompt_ends_the_name_without_a_result() {
        let mut run = CommandRun::default();
        let t0 = Instant::now();
        run.on_mark(started("pnpm dev"), t0);
        assert_eq!(run.on_mark(CommandMark::Prompt, t0), Some(Change::Cleared));
        assert_eq!(run.running(), None);
        assert_eq!(run.on_mark(CommandMark::Prompt, t0), None);
    }

    #[test]
    fn unknown_forgets_the_running_command() {
        let mut run = CommandRun::default();
        let t0 = Instant::now();
        run.on_mark(started("pnpm dev"), t0);
        assert_eq!(run.on_mark(CommandMark::Unknown, t0), Some(Change::Cleared));
        assert_eq!(run.running(), None);
        assert_eq!(run.on_mark(CommandMark::Unknown, t0), None);
        let end = CommandMark::Ended { exit: Some(0) };
        assert_eq!(run.on_mark(end, t0), None);
    }

    #[test]
    fn a_second_start_replaces_the_first() {
        let mut run = CommandRun::default();
        let t0 = Instant::now();
        run.on_mark(started("pnpm dev"), t0);
        let t1 = t0 + Duration::from_secs(5);
        assert_eq!(
            run.on_mark(started("cargo test"), t1),
            Some(Change::Started(Some("cargo test".to_string())))
        );
        assert_eq!(run.running(), Some("cargo test"));
        let end = CommandMark::Ended { exit: Some(1) };
        assert_eq!(
            run.on_mark(end, t1 + Duration::from_secs(2)),
            Some(Change::Ended(Finished {
                short: Some("cargo test".to_string()),
                ending: Ending::Failed(1),
                ran: Duration::from_secs(2),
            }))
        );
    }

    #[test]
    fn a_command_with_no_short_form_still_ends() {
        let mut run = CommandRun::default();
        let t0 = Instant::now();
        assert_eq!(
            run.on_mark(CommandMark::Started { short: None }, t0),
            Some(Change::Started(None))
        );
        assert_eq!(run.running(), None);
        let end = CommandMark::Ended { exit: Some(0) };
        let Some(Change::Ended(done)) = run.on_mark(end, t0 + Duration::from_secs(3)) else {
            panic!("a command with no short form still ends");
        };
        assert_eq!(done.short, None);
        assert_eq!(done.name(), "command");
        assert_eq!(done.listed(), "command · Done in 3s");
    }
}
