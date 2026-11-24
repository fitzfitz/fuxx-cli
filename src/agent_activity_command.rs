//! Reading a shell command for the activity view (the agent activity chore): which files it
//! reads, searches or changes, whether it runs a check (tests, a build, a lint) and on what scope.
//! Pure and generic: providers call it for their Bash / run_command tools.

use crate::agent_activity::{Check, CheckWhat, How, Outcome, ToolKind, Touch, derived_words};
use crate::agent_ask::cut;
use crate::agent_detect::{is_assignment, unquote};
use crate::command_name::{WRAPPERS, past_wrapper, short_form};

/// What a command does, as far as its words tell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    pub kind: ToolKind,
    pub touches: Vec<Touch>,
    /// The check it runs: what, and its normalised scope. Several checks in one command are one
    /// compound check: the first one's kind, their scopes joined by ` && ` (so only the same
    /// command clears it); a `cd` before a check is part of its scope (`cd crates/a && cargo test`).
    pub check: Option<(CheckWhat, String)>,
    /// The command's exit code is the check's own: every check is last or followed only by
    /// `&&`, and none feeds a pipe (`; echo done`, `|| true` and `| tail` hide it).
    pub exit_trusted: bool,
    pub derived: String,
}

/// The longest command a listing's title names, in characters.
const TITLE_WORDS_MAX: usize = 60;
/// Programs that run the script they are given (a heredoc, or `-c` / `-e` code).
const INTERPRETERS: [&str; 11] = [
    "python", "python3", "node", "ruby", "perl", "sh", "bash", "zsh", "deno", "bun", "python2",
];
const READERS: [&str; 7] = ["cat", "head", "tail", "less", "wc", "nl", "bat"];
const SEARCHERS: [&str; 3] = ["rg", "grep", "ag"];
const LISTERS: [&str; 3] = ["find", "fd", "ls"];
/// Output flags after a bare `--` (the test binary's) a scope leaves out; the `=`-less ones of
/// `OUTPUT_VALUE_FLAGS` take the next word.
const BINARY_OUTPUT_FLAGS: [&str; 4] = ["--nocapture", "--show-output", "-q", "--quiet"];
const OUTPUT_VALUE_FLAGS: [&str; 3] = ["--color", "--test-threads", "--format"];
/// Flags after a bare `--` whose next word is their value, not a filter (`--skip api`, `-D
/// warnings`).
const BINARY_VALUE_FLAGS: [&str; 10] = [
    "--skip",
    "-D",
    "-W",
    "-A",
    "-F",
    "--deny",
    "--warn",
    "--allow",
    "--forbid",
    "--cap-lints",
];
/// Extensions a script's string literal must end in to be read as a file when it holds no `/`.
const KNOWN_EXTS: [&str; 36] = [
    "rs", "toml", "lock", "py", "js", "mjs", "cjs", "ts", "tsx", "jsx", "json", "yaml", "yml",
    "md", "txt", "sh", "go", "c", "h", "cc", "cpp", "hpp", "java", "kt", "swift", "rb", "html",
    "css", "scss", "xml", "sql", "csv", "ini", "cfg", "conf", "log",
];
/// Output flags a check's scope leaves out: they change how it prints, not what it checks.
const OUTPUT_FLAGS: [&str; 7] = [
    "-q",
    "--quiet",
    "-v",
    "-vv",
    "--verbose",
    "--no-fail-fast",
    "-s",
];
/// Flags whose next word is their value (not a filter).
const VALUE_FLAGS: [&str; 12] = [
    "--bin",
    "-p",
    "--package",
    "--test",
    "--features",
    "-F",
    "--example",
    "--target",
    "--manifest-path",
    "-j",
    "--jobs",
    "--profile",
];
/// Search flags whose next word is their value (not the pattern or a file).
const SEARCH_VALUE_FLAGS: [&str; 14] = [
    "-e",
    "-f",
    "-g",
    "--glob",
    "-t",
    "--type",
    "-T",
    "-A",
    "-B",
    "-C",
    "-m",
    "--max-count",
    "--include",
    "--exclude",
];

/// What follows a segment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Sep {
    /// `&&`.
    And,
    /// `|` or `|&`.
    Pipe,
    /// `;`, `||` or a new line: the next command's exit replaces this one's.
    Then,
    /// `&`: run in the background, its exit never seen.
    Background,
    /// Nothing: the last command.
    #[default]
    End,
}

/// One command between `&&`, `||`, `;`, `|` or a new line: its words (unquoted), its `>` / `>>`
/// targets, the heredoc it reads, and what follows it.
#[derive(Default)]
struct Segment {
    words: Vec<String>,
    outs: Vec<String>,
    body: Option<String>,
    sep: Sep,
}

/// `path` under the folder a `cd` went to, when it is relative.
fn under(dir: Option<&str>, path: &str) -> String {
    match dir {
        Some(d) if !path.starts_with(['/', '~']) => {
            let path = path.strip_prefix("./").unwrap_or(path);
            format!("{}/{path}", d.trim_end_matches('/'))
        }
        _ => path.to_string(),
    }
}

/// What reading `command` finds. An unclosed quote or nothing readable is a plain run.
pub fn read_command(command: &str) -> Reading {
    let plain = || Reading {
        kind: ToolKind::Execute,
        touches: Vec::new(),
        check: None,
        exit_trusted: true,
        derived: derived_words(
            ToolKind::Execute,
            &[],
            None,
            &short_form(command).unwrap_or_default(),
        ),
    };
    let Some(segments) = segments(command) else {
        return plain();
    };
    let mut touches: Vec<Touch> = Vec::new();
    let mut add = |path: &str, how: How| {
        let t = Touch {
            path: path.to_string(),
            how,
        };
        if !touches.contains(&t) {
            touches.push(t);
        }
    };
    let changed = || How::Changed {
        plus: None,
        minus: None,
        at: None,
    };
    // Each check: its kind, its scope, the folder a `cd` before it went to, and its segment.
    let mut checks: Vec<(CheckWhat, String, Option<String>, usize)> = Vec::new();
    let mut dir: Option<String> = None;
    let (mut deleted, mut moved, mut change, mut script) = (false, false, false, false);
    let (mut searched, mut listed, mut read) = (false, false, false);
    let mut pattern = String::new();
    // A listing's command, in its words (`ls nope`): what a listing says it ran.
    let mut listing: Option<String> = None;
    for (si, seg) in segments.iter().enumerate() {
        let here = dir.clone();
        let at = |p: &str| under(here.as_deref(), p);
        for out in seg.outs.iter().filter(|o| looks_like_path(o)) {
            add(&at(out), changed());
            change = true;
        }
        let Some(pi) = program_at(&seg.words) else {
            continue;
        };
        let words = &seg.words[pi..];
        if let Some((what, runner)) = check_of(words) {
            checks.push((what, scope_of(runner), here.clone(), si));
            continue;
        }
        let program = words[0].rsplit('/').next().unwrap_or_default();
        let args = &words[1..];
        let paths = || args.iter().filter(|a| looks_like_path(a)).map(|a| at(a));
        match program {
            "cd" => {
                dir = args
                    .iter()
                    .find(|a| !a.starts_with('-'))
                    .map(|to| under(here.as_deref(), to));
            }
            p if READERS.contains(&p) => {
                read = true;
                paths().for_each(|a| add(&a, How::Read { lines: None }));
            }
            "sed" => {
                let (inplace, script_text, files) = sed_parts(args);
                let address = script_text.as_deref().map(sed_address).unwrap_or_default();
                for f in files.iter().map(|f| at(f)) {
                    if inplace {
                        change = true;
                        let at = match address {
                            Address::Line(n) => Some(n),
                            _ => None,
                        };
                        add(
                            &f,
                            How::Changed {
                                plus: None,
                                minus: None,
                                at,
                            },
                        );
                    } else {
                        read = true;
                        let lines = match address {
                            Address::Range(a, b) => Some((a, b)),
                            Address::Line(n) => Some((n, n)),
                            Address::None => None,
                        };
                        add(&f, How::Read { lines });
                    }
                }
            }
            p if SEARCHERS.contains(&p) => {
                searched = true;
                let (found, files) = search_parts(args);
                if pattern.is_empty() {
                    pattern = found.unwrap_or_default();
                }
                files
                    .into_iter()
                    .for_each(|f| add(&at(f), How::Searched { matches: None }));
            }
            p if LISTERS.contains(&p) => {
                listed = true;
                listing.get_or_insert_with(|| words.join(" "));
            }
            "tee" | "touch" => {
                paths().for_each(|a| add(&a, changed()));
                change = true;
            }
            "cp" => {
                if let Some(dest) = paths().next_back() {
                    add(&dest, changed());
                    change = true;
                }
            }
            "rm" => {
                paths().for_each(|a| add(&a, changed()));
                deleted = true;
            }
            "mv" => {
                paths().for_each(|a| add(&a, changed()));
                moved = true;
            }
            p if INTERPRETERS.contains(&p) => {
                let code = args
                    .iter()
                    .position(|a| a == "-c" || a == "-e")
                    .and_then(|i| args.get(i + 1));
                for text in seg.body.iter().chain(code) {
                    script = true;
                    literals(text)
                        .iter()
                        .filter(|l| script_path(l))
                        .for_each(|l| add(&at(l), How::ScriptTouched));
                }
            }
            _ => {}
        }
    }
    let exit_trusted = checks.iter().all(|&(.., si)| {
        segments[si].sep != Sep::Pipe
            && segments[si..]
                .iter()
                .all(|s| matches!(s.sep, Sep::And | Sep::End))
    });
    let check = compound(&checks);
    let kind = if check.is_some() {
        ToolKind::Execute
    } else if deleted {
        ToolKind::Delete
    } else if moved {
        ToolKind::Move
    } else if change {
        ToolKind::Edit
    } else if script {
        ToolKind::Execute
    } else if searched || listed {
        ToolKind::Search
    } else if read {
        ToolKind::Read
    } else {
        ToolKind::Execute
    };
    let probe = check
        .as_ref()
        .map(|(what, scope): &(CheckWhat, String)| Check {
            what: *what,
            scope: scope.clone(),
            outcome: Outcome::Unknown,
        });
    let derived = match kind {
        ToolKind::Search if !searched => derived_words(
            ToolKind::Execute,
            &[],
            None,
            &listing
                .map(|l| cut(&l, TITLE_WORDS_MAX))
                .unwrap_or_default(),
        ),
        ToolKind::Search => derived_words(kind, &touches, None, &pattern),
        ToolKind::Execute if check.is_none() && !script => {
            return Reading {
                kind,
                touches,
                ..plain()
            };
        }
        _ => derived_words(kind, &touches, probe.as_ref(), ""),
    };
    Reading {
        kind,
        touches,
        check,
        exit_trusted,
        derived,
    }
}

/// One check for a command's checks: the first one's kind and their scopes joined by ` && `,
/// each led by `cd <folder>` when a `cd` before it went somewhere new.
fn compound(checks: &[(CheckWhat, String, Option<String>, usize)]) -> Option<(CheckWhat, String)> {
    let (what, ..) = checks.first()?;
    let mut parts: Vec<String> = Vec::new();
    let mut was: Option<&str> = None;
    for (_, scope, dir, _) in checks {
        if let Some(d) = dir.as_deref().filter(|d| Some(*d) != was) {
            parts.push(format!("cd {d}"));
        }
        was = dir.as_deref();
        parts.push(scope.clone());
    }
    Some((*what, parts.join(" && ")))
}

/// The checks a scope names, in order (one for a plain scope, more for a compound one).
pub fn scope_checks(scope: &str) -> Vec<CheckWhat> {
    scope_parts(scope)
        .filter_map(|part| {
            let words: Vec<String> = part.split_whitespace().map(String::from).collect();
            check_of(&words).map(|(what, _)| what)
        })
        .collect()
}

/// A scope's check commands, without its `cd` parts.
fn scope_parts(scope: &str) -> impl Iterator<Item = &str> {
    scope.split(" && ").filter(|p| !p.starts_with("cd "))
}

/// Whether a script's string literal names a file: path-like, not a sed `s/…/…/` expression,
/// and holding a `/` or ending in a known extension (`'os.path'` and `'utf-8'` are words).
fn script_path(lit: &str) -> bool {
    let sed = lit.strip_prefix('s').and_then(|rest| {
        let d = rest.chars().next().filter(|c| !c.is_alphanumeric())?;
        (rest.matches(d).count() >= 3).then_some(())
    });
    looks_like_path(lit)
        && sed.is_none()
        && (lit.contains('/')
            || lit
                .rsplit_once('.')
                .is_some_and(|(_, ext)| KNOWN_EXTS.contains(&ext.to_ascii_lowercase().as_str())))
}

/// Where the program is among `words`: after `NAME=value` assignments and wrappers
/// (`sudo`, `env`, `time` …) with their options.
fn program_at(words: &[String]) -> Option<usize> {
    let mut i = 0;
    loop {
        while words.get(i).is_some_and(|w| is_assignment(w)) {
            i += 1;
        }
        let w = words.get(i)?;
        if WRAPPERS.contains(&w.rsplit('/').next().unwrap_or_default()) {
            i = past_wrapper(words, i);
            continue;
        }
        return (!w.is_empty()).then_some(i);
    }
}

/// The check `words` (from the program on) run, and the words from its runner on
/// (`npx jest api` → `jest api`; `python -m pytest x` → `pytest x`).
fn check_of(words: &[String]) -> Option<(CheckWhat, &[String])> {
    let program = words.first()?.rsplit('/').next().unwrap_or_default();
    let sub = |from: usize| {
        words[from..]
            .iter()
            .position(|w| !w.starts_with('-') && !w.starts_with('+'))
            .map(|i| (from + i, words[from + i].as_str()))
    };
    match program {
        "cargo" => match sub(1)?.1 {
            "test" | "nextest" => Some((CheckWhat::Tests, words)),
            "build" | "check" => Some((CheckWhat::Build, words)),
            "clippy" => Some((CheckWhat::Lint, words)),
            "fmt" if words.iter().any(|w| w == "--check") => Some((CheckWhat::Lint, words)),
            _ => None,
        },
        "pytest" | "py.test" | "jest" | "vitest" => Some((CheckWhat::Tests, words)),
        "python" | "python3" => {
            let m = words.iter().position(|w| w == "-m")?;
            (words.get(m + 1)? == "pytest").then(|| (CheckWhat::Tests, &words[m + 1..]))
        }
        "go" => match sub(1)?.1 {
            "test" => Some((CheckWhat::Tests, words)),
            "build" => Some((CheckWhat::Build, words)),
            "vet" => Some((CheckWhat::Lint, words)),
            _ => None,
        },
        "npx" | "pnpx" | "bunx" => check_of(&words[sub(1)?.0..]),
        "npm" | "pnpm" | "yarn" | "bun" => {
            let (at, word) = sub(1)?;
            match word {
                "test" | "t" => Some((CheckWhat::Tests, words)),
                "run" if words.get(at + 1).is_some_and(|w| w == "test") => {
                    Some((CheckWhat::Tests, words))
                }
                "exec" | "dlx" | "x" => check_of(&words[at + 1..]),
                _ if program != "npm" => check_of(&words[at..]),
                _ => None,
            }
        }
        _ => None,
    }
}

/// A check's scope, normalised so two runs of the same check compare equal: the runner, its
/// target flags and filters, and what follows a bare `--` (`-- cli::`, `--skip api`, `-D
/// warnings` narrow or tighten it); not output flags, redirects or leading assignments.
pub fn scope_of(words: &[String]) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut binary = false;
    let mut words = words
        .iter()
        .map(|w| unquote(w))
        .skip_while(|w| is_assignment(w));
    while let Some(w) = words.next() {
        if w == "--" && !binary {
            binary = true;
            kept.push(w);
            continue;
        }
        if binary {
            if BINARY_OUTPUT_FLAGS.contains(&w) || w.contains('>') || w.contains('<') {
                continue;
            }
            if let Some(flag) = OUTPUT_VALUE_FLAGS
                .iter()
                .find(|f| w == **f || w.starts_with(&format!("{f}=")))
            {
                if w == *flag {
                    words.next();
                }
                continue;
            }
            kept.push(w);
            continue;
        }
        if w.contains('>') || w.contains('<') || w.starts_with('+') || OUTPUT_FLAGS.contains(&w) {
            continue;
        }
        if w.starts_with("--color") || w.starts_with("--message-format") {
            if !w.contains('=') {
                words.next();
            }
            continue;
        }
        kept.push(w);
    }
    if kept.last() == Some(&"--") {
        kept.pop();
    }
    kept.join(" ")
}

/// A scope's filter: its words past the runner and its subcommands, without flags and their
/// values (`cargo test --bin fuxx cli::` → `cli::`; `-- --skip api` → nothing); a compound
/// scope's filters joined.
pub fn scope_filter(scope: &str) -> String {
    scope_parts(scope)
        .map(part_filter)
        .filter(|f| !f.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn part_filter(scope: &str) -> String {
    let mut words = scope.split_whitespace().skip(1).peekable();
    while words.peek().is_some_and(|w| {
        [
            "test", "build", "check", "clippy", "fmt", "vet", "run", "nextest",
        ]
        .contains(w)
    }) {
        words.next();
    }
    let mut kept = Vec::new();
    let mut binary = false;
    while let Some(w) = words.next() {
        if w == "--" {
            binary = true;
        } else if VALUE_FLAGS.contains(&w) && !binary || binary && BINARY_VALUE_FLAGS.contains(&w) {
            words.next();
        } else if !w.starts_with('-') {
            kept.push(w);
        }
    }
    kept.join(" ")
}

/// Whether a word names a file: not a flag, number, URL, glob or device, and either holds a `/`
/// or ends in an extension.
pub fn looks_like_path(word: &str) -> bool {
    if word.is_empty()
        || word.starts_with('-')
        || word.contains("://")
        || word.starts_with("/dev/")
        || word.parse::<f64>().is_ok()
        || word
            .chars()
            .any(|c| c.is_whitespace() || "*?[]{}$%=<>|&;()`'\"\\".contains(c))
    {
        return false;
    }
    if word.contains('/') {
        return word.chars().any(|c| c.is_alphanumeric());
    }
    match word.rsplit_once('.') {
        Some((stem, ext)) => {
            !ext.is_empty()
                && ext.len() <= 10
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && ext.chars().any(|c| c.is_ascii_alphabetic())
                && (!stem.is_empty() || ext.len() > 1)
        }
        None => false,
    }
}

#[derive(Clone, Copy, Default)]
enum Address {
    #[default]
    None,
    Line(u32),
    Range(u32, u32),
}

/// sed's `-i`, its script and its files.
fn sed_parts(args: &[String]) -> (bool, Option<String>, Vec<&String>) {
    let (mut inplace, mut script, mut files) = (false, None, Vec::new());
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        let letters = a
            .strip_prefix('-')
            .filter(|f| !f.starts_with('-'))
            .map(|f| {
                f.split(|c: char| !c.is_ascii_alphabetic())
                    .next()
                    .unwrap_or("")
            });
        if letters.is_some_and(|l| l.contains('i')) || a.starts_with("--in-place") {
            inplace = true;
            if it.peek().is_some_and(|n| n.is_empty()) {
                it.next(); // macOS `sed -i '' …`
            }
        } else if a == "-e" || a == "--expression" {
            script = it.next().cloned();
        } else if a.starts_with('-') || a.is_empty() {
        } else if script.is_none() {
            script = Some(a.clone());
        } else if looks_like_path(a) {
            files.push(a);
        }
    }
    (inplace, script, files)
}

/// A sed script's leading address: `543s/…/` → line 543; `80,400p` → lines 80–400.
fn sed_address(script: &str) -> Address {
    let digits = |s: &str| -> (Option<u32>, usize) {
        let n: String = s.chars().take_while(char::is_ascii_digit).collect();
        (n.parse().ok(), n.len())
    };
    let (Some(a), len) = digits(script) else {
        return Address::None;
    };
    let rest = &script[len..];
    match rest.strip_prefix(',') {
        Some(tail) => match digits(tail) {
            (Some(b), _) => Address::Range(a, b),
            _ => Address::Line(a),
        },
        None => Address::Line(a),
    }
}

/// A search's pattern and the files it searches.
fn search_parts(args: &[String]) -> (Option<String>, Vec<&String>) {
    let (mut pattern, mut files) = (None, Vec::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "-e" || a == "--regexp" {
            pattern = it.next().cloned();
        } else if SEARCH_VALUE_FLAGS.contains(&a.as_str()) {
            it.next();
        } else if a.starts_with('-') {
        } else if pattern.is_none() {
            pattern = Some(a.clone());
        } else if looks_like_path(a) {
            files.push(a);
        }
    }
    (pattern, files)
}

/// The quoted string literals in a script (one line each).
fn literals(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in code.lines() {
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            if c != '\'' && c != '"' {
                continue;
            }
            let mut lit = String::new();
            let mut closed = false;
            while let Some(d) = chars.next() {
                if d == '\\' {
                    chars.next();
                } else if d == c {
                    closed = true;
                    break;
                } else {
                    lit.push(d);
                }
            }
            if closed && !lit.is_empty() {
                out.push(lit);
            }
        }
    }
    out
}

/// `command` cut into segments, heredoc bodies kept apart; None for an unclosed quote.
fn segments(command: &str) -> Option<Vec<Segment>> {
    let (text, bodies) = cut_heredocs(command);
    let mut bodies = bodies.into_iter();
    let mut segs = Vec::new();
    let mut cur = Segment::default();
    let mut word = String::new();
    let mut has_word = false;
    let mut quote: Option<char> = None;
    #[derive(Clone, Copy)]
    enum Next {
        Out,
        Heredoc,
        Skip,
    }
    let mut pending: Option<Next> = None;
    let mut finish =
        |word: &mut String, has_word: &mut bool, cur: &mut Segment, pending: &mut Option<Next>| {
            if word.is_empty() && !*has_word {
                return;
            }
            let w = std::mem::take(word);
            *has_word = false;
            match pending.take() {
                Some(Next::Out) => cur.outs.push(w),
                Some(Next::Heredoc) => cur.body = bodies.next(),
                Some(Next::Skip) => {}
                None => cur.words.push(w),
            }
        };
    let end = |cur: &mut Segment, segs: &mut Vec<Segment>, sep: Sep| {
        let mut done = std::mem::take(cur);
        if !done.words.is_empty() || !done.outs.is_empty() || done.body.is_some() {
            done.sep = sep;
            segs.push(done);
        }
    };
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else if q == '"' && c == '\\' {
                if let Some(n) = chars.next() {
                    word.push(n);
                }
            } else {
                word.push(c);
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                has_word = true;
            }
            '\\' => match chars.next() {
                Some('\n') | None => {}
                Some(n) => word.push(n),
            },
            ' ' | '\t' | '\r' | '(' | ')' => {
                finish(&mut word, &mut has_word, &mut cur, &mut pending)
            }
            '\n' | ';' => {
                finish(&mut word, &mut has_word, &mut cur, &mut pending);
                end(&mut cur, &mut segs, Sep::Then);
            }
            '|' => {
                finish(&mut word, &mut has_word, &mut cur, &mut pending);
                let sep = match chars.peek() {
                    Some('|') => {
                        chars.next();
                        Sep::Then
                    }
                    Some('&') => {
                        chars.next();
                        Sep::Pipe
                    }
                    _ => Sep::Pipe,
                };
                end(&mut cur, &mut segs, sep);
            }
            '&' => {
                finish(&mut word, &mut has_word, &mut cur, &mut pending);
                match chars.peek() {
                    Some('&') => {
                        chars.next();
                        end(&mut cur, &mut segs, Sep::And);
                    }
                    Some('>') => {
                        chars.next();
                        if chars.peek() == Some(&'>') {
                            chars.next();
                        }
                        pending = Some(Next::Out);
                    }
                    _ => end(&mut cur, &mut segs, Sep::Background),
                }
            }
            '>' => {
                if !word.is_empty() && word.chars().all(|c| c.is_ascii_digit()) {
                    word.clear(); // the fd: `2>`
                } else {
                    finish(&mut word, &mut has_word, &mut cur, &mut pending);
                }
                if matches!(chars.peek(), Some('>') | Some('|')) {
                    chars.next();
                }
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
                if chars.peek() == Some(&'&') {
                    chars.next(); // `>&2`, `2>&1`: a copy of a stream, not a file
                    while chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '-')
                    {
                        chars.next();
                    }
                } else {
                    pending = Some(Next::Out);
                }
            }
            '<' => {
                finish(&mut word, &mut has_word, &mut cur, &mut pending);
                if chars.peek() == Some(&'<') {
                    chars.next();
                    if chars.peek() == Some(&'<') {
                        chars.next();
                        pending = Some(Next::Skip);
                    } else {
                        if chars.peek() == Some(&'-') {
                            chars.next();
                        }
                        pending = Some(Next::Heredoc);
                    }
                } else {
                    pending = Some(Next::Skip);
                }
            }
            _ => word.push(c),
        }
    }
    if quote.is_some() {
        return None;
    }
    finish(&mut word, &mut has_word, &mut cur, &mut pending);
    end(&mut cur, &mut segs, Sep::End);
    // Separators with nothing after them (a closing new line, a trailing `;`) end it.
    if let Some(last) = segs.last_mut()
        && last.sep == Sep::Then
    {
        last.sep = Sep::End;
    }
    Some(segs)
}

/// `command` without its heredoc bodies, and the bodies in the order their `<<` comes.
fn cut_heredocs(command: &str) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut bodies = Vec::new();
    let mut quote: Option<char> = None;
    let mut lines = command.split('\n');
    while let Some(line) = lines.next() {
        let starts = heredoc_starts(line, &mut quote);
        text.push_str(line);
        text.push('\n');
        for (delimiter, dash) in starts {
            let mut body = String::new();
            for l in lines.by_ref() {
                let probe = if dash { l.trim_start_matches('\t') } else { l };
                if probe.trim() == delimiter {
                    break;
                }
                body.push_str(l);
                body.push('\n');
            }
            bodies.push(body);
        }
    }
    (text, bodies)
}

/// The heredocs a line opens (`<<WORD`, `<<'WORD'`, `<<-WORD`): each delimiter and whether it
/// is the `<<-` kind. `quote` carries an open quote across lines.
fn heredoc_starts(line: &str, quote: &mut Option<char>) -> Vec<(String, bool)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = *quote {
            if c == q {
                *quote = None;
            } else if q == '"' && c == '\\' {
                i += 1;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => *quote = Some(c),
            '\\' => i += 1,
            '<' if chars.get(i + 1) == Some(&'<') && chars.get(i + 2) != Some(&'<') => {
                i += 2;
                let dash = chars.get(i) == Some(&'-');
                if dash {
                    i += 1;
                }
                while chars.get(i) == Some(&' ') {
                    i += 1;
                }
                let mut delimiter = String::new();
                if let Some(&q) = chars.get(i).filter(|c| **c == '\'' || **c == '"') {
                    i += 1;
                    while let Some(&d) = chars.get(i) {
                        i += 1;
                        if d == q {
                            break;
                        }
                        delimiter.push(d);
                    }
                } else {
                    while let Some(&d) = chars.get(i) {
                        if d.is_whitespace() || ";|&<>()".contains(d) {
                            break;
                        }
                        if d != '\\' {
                            delimiter.push(d);
                        }
                        i += 1;
                    }
                }
                if !delimiter.is_empty() {
                    out.push((delimiter, dash));
                }
                continue;
            }
            '<' if chars.get(i + 1) == Some(&'<') => i += 2, // `<<<`
            _ => {}
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_activity::{CheckWhat, How, ToolKind, Touch};

    fn touch(p: &str, how: How) -> Touch {
        Touch {
            path: p.into(),
            how,
        }
    }
    const CHANGED: How = How::Changed {
        plus: None,
        minus: None,
        at: None,
    };

    #[test]
    fn a_heredoc_body_is_not_read_as_commands() {
        let r = read_command(
            "python3 - <<'EOF'\nimport os\nos.remove('x.txt')\nEOF\ncargo test --bin fuxx cli:: 2>&1 | tail -20",
        );
        assert_eq!(
            r.check,
            Some((CheckWhat::Tests, "cargo test --bin fuxx cli::".into()))
        );
        assert!(!r.exit_trusted);
        assert_eq!(r.kind, ToolKind::Execute);
        assert!(
            r.touches
                .iter()
                .all(|t| !matches!(t.how, How::Changed { .. }))
        );
    }

    #[test]
    fn a_heredoc_that_only_reads_touches_by_script_never_changes() {
        let r = read_command(
            "python3 - <<'EOF'\nsrc = open('src/api_client.rs').read()\nprint(src[:400])\nEOF",
        );
        assert_eq!(
            r.touches,
            vec![touch("src/api_client.rs", How::ScriptTouched)]
        );
        assert_eq!(r.kind, ToolKind::Execute);
        assert_eq!(r.derived, "Ran a script on api_client.rs");
    }

    #[test]
    fn a_cat_heredoc_writes_its_target_and_its_body_names_nothing() {
        let r = read_command("cat > notes.txt <<'EOF'\nsee src/api.rs\nEOF");
        assert_eq!(r.touches, vec![touch("notes.txt", CHANGED)]);
        assert_eq!(r.kind, ToolKind::Edit);
    }

    #[test]
    fn verbs_map_to_reads_searches_changes_and_checks() {
        for (cmd, kind, touches) in [
            (
                "sed -n '80,400p' src/api_client.rs",
                ToolKind::Read,
                vec![touch(
                    "src/api_client.rs",
                    How::Read {
                        lines: Some((80, 400)),
                    },
                )],
            ),
            (
                "cat src/cli.rs | head -40",
                ToolKind::Read,
                vec![touch("src/cli.rs", How::Read { lines: None })],
            ),
            (
                "rg -n METHODS src/help_commands.rs",
                ToolKind::Search,
                vec![touch(
                    "src/help_commands.rs",
                    How::Searched { matches: None },
                )],
            ),
            (
                "sed -i '543s/old/new/' src/api.rs",
                ToolKind::Edit,
                vec![touch(
                    "src/api.rs",
                    How::Changed {
                        plus: None,
                        minus: None,
                        at: Some(543),
                    },
                )],
            ),
            (
                "echo done > out.txt 2>/dev/null",
                ToolKind::Edit,
                vec![touch("out.txt", CHANGED)],
            ),
            (
                "rm src/old.rs",
                ToolKind::Delete,
                vec![touch("src/old.rs", CHANGED)],
            ),
            (
                "mv src/a.rs src/b.rs",
                ToolKind::Move,
                vec![touch("src/a.rs", CHANGED), touch("src/b.rs", CHANGED)],
            ),
        ] {
            let r = read_command(cmd);
            assert_eq!((r.kind, r.touches), (kind, touches), "{cmd}");
        }
        for (cmd, what, scope) in [
            (
                "cargo build 2>&1 | tail -5",
                CheckWhat::Build,
                "cargo build",
            ),
            (
                "cargo clippy --all-targets -- -D warnings",
                CheckWhat::Lint,
                "cargo clippy --all-targets -- -D warnings",
            ),
            ("cargo fmt --check", CheckWhat::Lint, "cargo fmt --check"),
            (
                "pytest tests/test_api.py -q",
                CheckWhat::Tests,
                "pytest tests/test_api.py",
            ),
            ("npx jest api", CheckWhat::Tests, "jest api"),
            ("npx vitest run api", CheckWhat::Tests, "vitest run api"),
            ("go test ./api/...", CheckWhat::Tests, "go test ./api/..."),
        ] {
            assert_eq!(
                read_command(cmd).check,
                Some((what, scope.to_string())),
                "{cmd}"
            );
        }
    }

    #[test]
    fn scope_drops_output_flags_and_keeps_runner_target_and_filter() {
        let w = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        assert_eq!(
            scope_of(&w("cargo test -q --bin fuxx cli:: -- --nocapture")),
            "cargo test --bin fuxx cli::"
        );
        assert_eq!(
            scope_of(&w("RUST_LOG=debug cargo test --bin fuxx cli::")),
            "cargo test --bin fuxx cli::"
        );
        assert_ne!(
            scope_of(&w("cargo test --bin fuxx cli::")),
            scope_of(&w("cargo test --bin fuxx api::"))
        );
    }

    #[test]
    fn a_checks_exit_is_trusted_only_when_nothing_after_it_can_replace_it() {
        for (cmd, trusted) in [
            ("cargo test", true),
            ("cargo test && echo ok", true),
            ("cd crates/a && cargo test", true),
            ("cargo fmt --check; echo done", false),
            ("cargo test || true", false),
            ("cargo test\necho done", false),
            ("cargo test 2>&1 | tail -20", false),
            ("cargo test &", false),
        ] {
            assert_eq!(read_command(cmd).exit_trusted, trusted, "{cmd}");
        }
    }

    #[test]
    fn words_after_a_bare_dashdash_stay_in_the_scope_but_output_flags() {
        let w = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        for (wide, narrow) in [
            ("cargo test", "cargo test -- cli::"),
            ("cargo test", "cargo test -- --skip api"),
            ("cargo test", "cargo test -- --ignored"),
            ("cargo test", "cargo test -- --exact"),
            (
                "cargo clippy --all-targets -- -D warnings",
                "cargo clippy --all-targets",
            ),
        ] {
            assert_ne!(scope_of(&w(wide)), scope_of(&w(narrow)), "{narrow}");
        }
        assert_eq!(
            scope_of(&w(
                "cargo test cli:: -- --nocapture --show-output --color=always -q --test-threads=1 --format pretty"
            )),
            "cargo test cli::"
        );
        assert_eq!(
            scope_of(&w("cargo test -- --test-threads 2 --color never")),
            "cargo test"
        );
        assert_eq!(scope_filter("cargo test -- --skip api"), "");
        assert_eq!(scope_filter("cargo test -- cli::"), "cli::");
        assert_eq!(
            scope_filter("cargo clippy --all-targets -- -D warnings"),
            ""
        );
    }

    #[test]
    fn several_checks_in_one_command_share_one_compound_scope() {
        assert_eq!(
            read_command("cargo clippy --all-targets -- -D warnings && cargo test").check,
            Some((
                CheckWhat::Lint,
                "cargo clippy --all-targets -- -D warnings && cargo test".into()
            ))
        );
        assert_eq!(
            read_command("cargo test -p a && cargo test -p b").check,
            Some((
                CheckWhat::Tests,
                "cargo test -p a && cargo test -p b".into()
            ))
        );
        assert_eq!(
            read_command("cargo clippy && cargo test").derived,
            "Ran lint + tests"
        );
    }

    #[test]
    fn a_cd_scopes_the_check_and_prefixes_relative_paths() {
        let a = read_command("cd crates/a && cargo test");
        let b = read_command("cd crates/b && cargo test");
        assert_ne!(a.check, b.check);
        assert_eq!(
            a.check,
            Some((CheckWhat::Tests, "cd crates/a && cargo test".into()))
        );
        assert_eq!(
            read_command("cd crates/a && sed -i '3s/x/y/' src/lib.rs").touches,
            vec![touch(
                "crates/a/src/lib.rs",
                How::Changed {
                    plus: None,
                    minus: None,
                    at: Some(3),
                },
            )]
        );
        assert_eq!(
            read_command("cd /tmp/fx/other && cat x.rs && cat /etc/hosts.txt").touches,
            vec![
                touch("/tmp/fx/other/x.rs", How::Read { lines: None }),
                touch("/etc/hosts.txt", How::Read { lines: None }),
            ]
        );
        assert_eq!(
            read_command("cd crates/a && cargo clippy && cargo test").check,
            Some((
                CheckWhat::Lint,
                "cd crates/a && cargo clippy && cargo test".into()
            ))
        );
    }

    #[test]
    fn a_listing_says_the_command_it_ran() {
        assert_eq!(read_command("ls nope").derived, "Ran ls nope");
        assert_eq!(read_command("ls").derived, "Ran ls");
        assert_eq!(read_command("ls nope").kind, ToolKind::Search);
    }

    #[test]
    fn timeout_wraps_a_check_and_its_seconds_are_skipped() {
        assert_eq!(
            read_command("timeout 600 cargo test --bin fuxx").check,
            Some((CheckWhat::Tests, "cargo test --bin fuxx".into()))
        );
    }

    #[test]
    fn combined_sed_flags_with_i_edit_in_place() {
        for cmd in ["sed -Ei 's/a/b/' src/api.rs", "sed -ni '3p' src/api.rs"] {
            assert_eq!(read_command(cmd).kind, ToolKind::Edit, "{cmd}");
        }
        assert_eq!(read_command("sed -n '3p' src/api.rs").kind, ToolKind::Read);
    }

    #[test]
    fn windows_line_endings_leave_no_carriage_return_in_words() {
        assert_eq!(
            read_command("cat src/api.rs\r\ncargo test\r\n").touches,
            vec![touch("src/api.rs", How::Read { lines: None })]
        );
        assert_eq!(
            read_command("cargo test\r").check,
            Some((CheckWhat::Tests, "cargo test".into()))
        );
    }

    #[test]
    fn a_scripts_sed_patterns_and_plain_words_are_not_files() {
        let r = read_command(
            "python3 - <<'EOF'\nimport re, os.path\nsubprocess.run(['sed', '-i', 's/a/b/g', 'src/api.rs'])\nx = 'utf-8'\ny = 'os.path'\nEOF",
        );
        assert_eq!(r.touches, vec![touch("src/api.rs", How::ScriptTouched)]);
    }

    #[test]
    fn an_unclosed_quote_or_garbage_reads_as_a_plain_run() {
        let r = read_command("echo 'unclosed");
        assert_eq!(r.kind, ToolKind::Execute);
        assert!(r.touches.is_empty() && r.check.is_none());
        for odd in ["", "<<", "| |", "python3 - <<EOF"] {
            let _ = read_command(odd);
        } // no panic
    }

    #[test]
    fn paths_look_like_files_not_flags_numbers_or_urls() {
        for yes in ["src/api.rs", "notes.txt", "./src/cli.rs"] {
            assert!(looks_like_path(yes), "{yes}");
        }
        for no in [
            "--bin",
            "1.5",
            "https://example.com/a.js",
            "-n",
            "/dev/null",
        ] {
            assert!(!looks_like_path(no), "{no}");
        }
    }
}
