//! What leaves the terminal of a command line: its short form, and nothing more.
//!
//! A command line may hold secrets (`API_KEY=… npm run deploy --token …`), so only the program, at
//! most one plain word after it and an ellipsis when another command follows may be shown
//! anywhere: a pane's name, a banner, the 🔔 list, Activity. Mark text is untrusted, so the
//! result is cleaned and capped here.

use crate::agent_detect::{is_assignment, unquote};

/// The longest program name kept, in characters.
pub const PROGRAM_CAP: usize = 32;
/// The longest subcommand word kept, in characters.
pub const WORD_CAP: usize = 24;
/// What a banner calls a command that has no short form.
#[allow(dead_code)] // until M11q Task 3
pub const NO_SHORT: &str = "command";

/// Programs that run the next command: they and their options are skipped.
const WRAPPERS: [&str; 6] = ["sudo", "env", "time", "nohup", "command", "exec"];
/// Words after which one more word is the thing run (`npm run build`).
const RUNNERS: [&str; 4] = ["run", "exec", "x", "dlx"];

/// The first command's raw words (quotes kept) and whether another command follows it. `None`
/// for an unclosed quote anywhere in the line.
fn first_command(line: &str) -> Option<(Vec<String>, bool)> {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut stopped = false;
    let mut quiet = false; // stopped by a lone `&`: what follows is not "another command"
    let mut more = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
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
                word.extend(chars.next());
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
        let stops = matches!(c, ';' | '|' | '\n') || (c == '&' && chars.peek() == Some(&'&'));
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
        word.clear();
        if stops {
            stopped = true;
            if c == '&' {
                chars.next();
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if !stopped && !word.is_empty() && word != "&" {
        words.push(word);
    }
    Some((words, more && !quiet))
}

/// A word of ASCII letters, digits and `-_.:`, 1 to `WORD_CAP` characters, not starting with `-`.
fn plain(word: &str) -> bool {
    (1..=WORD_CAP).contains(&word.chars().count())
        && !word.starts_with('-')
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

/// The short form of a command line: the program, a plain word after it (and after `run`, `exec`,
/// `x` or `dlx` one more), and ` …` when another command follows. `None` when no program can be
/// named.
#[allow(dead_code)] // until M11q Task 3
pub fn short_form(line: &str) -> Option<String> {
    let (words, more) = first_command(line)?;
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
    let mut out = name;
    if let Some(w) = words.get(i).filter(|w| plain(w)) {
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
            ("sudo -E ./scripts/build.sh prod", Some("build.sh prod")),
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
            ("sleep 5", Some("sleep 5")),
            ("sleep 8; true", Some("sleep 8 …")),
            ("sh -c 'sleep 4; exit 3'", Some("sh")),
            ("FOO=secret sleep 6 --x", Some("sleep 6")),
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
            ("sleep 5 &", Some("sleep 5")),
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
}
