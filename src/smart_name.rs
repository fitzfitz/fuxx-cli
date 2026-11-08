//! What a terminal is called (M11i, note 60 §1): your name for it, else what runs there and
//! where (`claude · fuxx-cli`), else where (`fuxx-cli`, `~`), else for SSH the host
//! (`devbox · src`), else the preset and the folder, else the program's own title, else ╭∩╮.
//!
//! All pure: `program_name` turns a command line from `ps` into a short name, `folder_name` a
//! folder into its last part, `smart_name` applies the rules in order, and `Stable` keeps a
//! terminal's program from flickering (a change needs the same answer twice in a row, except
//! for a known agent). The Workspace (`Programs`, `terminal_label`) feeds them.

use std::path::Path;

use crate::groups;

/// Between the program and the folder.
pub const SEPARATOR: &str = " · ";
/// A program's name is at most this many characters (the last then `…`).
pub const MAX_PROGRAM: usize = 24;

/// What runs in a terminal, as far as is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramState {
    /// A program other than the shell is in front: its short name (`program_name`).
    Running(String),
    /// The shell is at its prompt.
    Prompt,
    /// Not looked up (yet), or it can't be: an SSH or background-service terminal.
    Unknown,
}

/// What `smart_name` knows of a terminal.
pub struct NameFacts<'a> {
    /// Your name for it (blank counts as none).
    pub user: Option<&'a str>,
    pub program: ProgramState,
    /// Its working folder (a remote one for SSH: only as its shell reported it).
    pub folder: Option<&'a Path>,
    /// Your home folder, called `~`.
    pub home: Option<&'a Path>,
    /// The host of an SSH terminal.
    pub ssh_host: Option<&'a str>,
    /// The preset that started it.
    pub preset: Option<&'a str>,
    /// The program's own title (OSC 0/2).
    pub title: Option<&'a str>,
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
/// Programs named after the script they run (`python3.N` too; see `is_interpreter`).
const INTERPRETERS: &[&str] = &[
    "node", "python", "python3", "ruby", "perl", "php", "bun", "deno", "sh", "bash", "zsh", "fish",
];
/// Tools named with their subcommand (`cargo test`; after `run` or `exec`, one word more).
const SUBCOMMAND_TOOLS: &[&str] = &[
    "npm", "pnpm", "yarn", "bun", "npx", "cargo", "git", "go", "make", "just", "docker", "uv",
    "mise",
];

/// A command line from `ps` as a short name: a known agent's name, a script's (for an
/// interpreter), a tool and its subcommand, or else the program's file name. Cut at a newline
/// (`ps` writes it `\012`) and at `MAX_PROGRAM` characters.
pub fn program_name(args: &str) -> String {
    let line = args.split("\\012").next().unwrap_or_default();
    let line = line.split('\n').next().unwrap_or_default();
    let words: Vec<&str> = line.split_whitespace().collect();
    let Some((&first, rest)) = words.split_first() else {
        return String::new();
    };
    let name = agent_in(&words).map_or_else(|| named(first, rest), str::to_string);
    cut(name)
}

/// A known agent anywhere in `words`, by its name or by its package's path.
fn agent_in(words: &[&str]) -> Option<&'static str> {
    words.iter().find_map(|word| {
        let name = file_name(word);
        AGENTS.iter().copied().find(|a| *a == name).or_else(|| {
            AGENT_PACKAGES
                .iter()
                .find(|(package, _)| word.contains(package))
                .map(|(_, agent)| *agent)
        })
    })
}

/// A program's file name: the folder part and a login shell's leading `-` dropped.
fn file_name(word: &str) -> &str {
    let name = word.rsplit('/').next().unwrap_or(word);
    name.trim_start_matches('-')
}

fn is_interpreter(name: &str) -> bool {
    INTERPRETERS.contains(&name)
        || name
            .strip_prefix("python3.")
            .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))
}

/// `program` run with `rest` as a name: the script an interpreter runs (named in turn, so
/// `node …/npm run dev` is `npm run dev`), else the tool and its subcommand, else the program.
fn named(program: &str, rest: &[&str]) -> String {
    let program = file_name(program);
    if is_interpreter(program) {
        // The script is the very next word (a flag first, as `python3 -m`, names no script).
        // bun runs scripts and subcommands alike: a script is a path or has an extension.
        let script = rest.first().filter(|w| {
            !w.starts_with('-') && (program != "bun" || w.contains('/') || w.contains('.'))
        });
        if let Some(script) = script {
            let stem = Path::new(script).file_stem().map_or_else(
                || file_name(script).to_string(),
                |s| s.to_string_lossy().into(),
            );
            return with_subcommand(&stem, &rest[1..]);
        }
    }
    with_subcommand(program, rest)
}

/// `tool` and its subcommand when it's one of `SUBCOMMAND_TOOLS`; else `tool` alone.
fn with_subcommand(tool: &str, rest: &[&str]) -> String {
    let mut name = tool.to_string();
    if !SUBCOMMAND_TOOLS.contains(&tool) {
        return name;
    }
    let mut words = rest.iter().filter(|w| !w.starts_with('-'));
    if let Some(sub) = words.next() {
        name = format!("{name} {sub}");
        if matches!(*sub, "run" | "exec")
            && let Some(next) = words.next()
        {
            name = format!("{name} {next}");
        }
    }
    name
}

/// `name` cut to `MAX_PROGRAM` characters, the last of them `…`.
fn cut(name: String) -> String {
    if name.chars().count() <= MAX_PROGRAM {
        return name;
    }
    let mut cut: String = name.chars().take(MAX_PROGRAM - 1).collect();
    cut.push('…');
    cut
}

/// Whether `name` (a `program_name`) is one of the coding agents fuxx knows.
pub fn is_known_agent(name: &str) -> bool {
    AGENTS.contains(&name)
}

/// A folder as a name: its last part, `~` for `home`, `/` for the root.
pub fn folder_name(folder: &Path, home: Option<&Path>) -> String {
    if home.is_some_and(|home| home == folder) {
        return "~".into();
    }
    folder
        .file_name()
        .map_or_else(|| "/".into(), |name| name.to_string_lossy().into_owned())
}

/// The terminal's name: the first rule that applies (note 60 §1).
pub fn smart_name(f: &NameFacts) -> String {
    // 1. Yours, exactly.
    if let Some(user) = f.user.filter(|u| !u.trim().is_empty()) {
        return user.to_string();
    }
    let folder = f.folder.map(|folder| folder_name(folder, f.home));
    let at_folder = |head: &str| match &folder {
        Some(folder) => format!("{head}{SEPARATOR}{folder}"),
        None => head.to_string(),
    };
    match (&f.program, &folder) {
        // 2. What runs, and where.
        (ProgramState::Running(program), _) if !program.is_empty() => return at_folder(program),
        // 3. At the prompt: where.
        (ProgramState::Prompt, Some(folder)) => return folder.clone(),
        _ => {}
    }
    // 4. SSH: the host, and the folder its shell reported.
    if let Some(host) = f.ssh_host {
        return at_folder(host);
    }
    // 5. Not known what runs: the preset that started it, and where; else where.
    if let Some(preset) = f.preset.filter(|p| !p.trim().is_empty()) {
        return at_folder(preset);
    }
    if let Some(folder) = folder {
        return folder;
    }
    // 6. Nothing known: the program's own title, else ╭∩╮.
    f.title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(groups::DEFAULT_NAME)
        .to_string()
}

/// The program's title, when it says something `name` doesn't (shown under the name in a
/// tooltip); none when it is blank or the same.
pub fn extra_title<'a>(name: &str, title: Option<&'a str>) -> Option<&'a str> {
    title.filter(|t| !t.trim().is_empty() && t.trim() != name.trim())
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

#[cfg(test)]
mod tests {
    use super::*;
    use ProgramState::*;

    fn run(name: &str) -> ProgramState {
        Running(name.into())
    }

    #[test]
    fn programs_are_named_short() {
        let cases = [
            ("claude", "claude"),
            (
                "node /opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js",
                "claude",
            ),
            ("node /usr/local/bin/npm run dev", "npm run dev"),
            ("pnpm dev", "pnpm dev"),
            ("cargo test --release", "cargo test"),
            ("/usr/bin/vim src/main.rs", "vim"),
            ("python3 -m http.server 8000", "python3"),
            ("python3 manage.py runserver", "manage"),
            ("bash ./build.sh", "build"),
            ("bash", "bash"),
            ("-zsh", "zsh"),
            ("codex --full-auto", "codex"),
            ("foo\\012bar", "foo"),
            (
                "node /usr/local/lib/node_modules/@openai/codex/bin/codex.js",
                "codex",
            ),
            ("node /x/@google/gemini-cli/dist/index.js", "gemini"),
            ("/opt/homebrew/bin/aider --model x", "aider"),
            ("make -j8 all", "make all"),
            ("git rebase -i main", "git rebase"),
            ("yarn exec tsc", "yarn exec tsc"),
            ("python3.12 app.py", "app"),
            ("bun run dev", "bun run dev"),
            ("bun server.ts", "server"),
            ("/bin/zsh -l", "zsh"),
        ];
        for (args, name) in cases {
            assert_eq!(program_name(args), name, "{args}");
        }
    }

    #[test]
    fn a_long_name_is_cut_to_24_characters() {
        let long = "a".repeat(40);
        let name = program_name(&long);
        assert_eq!(name.chars().count(), MAX_PROGRAM);
        assert!(name.ends_with('…'), "{name}");
        assert!(name.starts_with(&"a".repeat(23)));
        assert_eq!(program_name(&"b".repeat(24)), "b".repeat(24));
    }

    #[test]
    fn the_known_agents_are_known() {
        for agent in [
            "claude", "codex", "opencode", "aider", "gemini", "goose", "amp",
        ] {
            assert!(is_known_agent(agent), "{agent}");
        }
        for other in ["make", "vim", "npm run dev", "zsh", ""] {
            assert!(!is_known_agent(other), "{other}");
        }
    }

    #[test]
    fn folders_are_named_by_their_last_part() {
        let home = Some(Path::new("/Users/me"));
        assert_eq!(
            folder_name(Path::new("/Users/me/Kerja/proj/fuxx-cli"), home),
            "fuxx-cli"
        );
        assert_eq!(folder_name(Path::new("/Users/me"), home), "~");
        assert_eq!(folder_name(Path::new("/Users/me/"), home), "~");
        assert_eq!(folder_name(Path::new("/"), home), "/");
        assert_eq!(folder_name(Path::new("/Users/me"), None), "me");
    }

    fn facts<'a>() -> NameFacts<'a> {
        NameFacts {
            user: None,
            program: Unknown,
            folder: None,
            home: Some(Path::new("/Users/me")),
            ssh_host: None,
            preset: None,
            title: None,
        }
    }

    #[test]
    fn the_rules_apply_in_order() {
        let repo = Some(Path::new("/Users/me/Kerja/proj/fuxx-cli"));
        // 1. Your name wins over a running program.
        let named = NameFacts {
            user: Some("api"),
            program: run("claude"),
            folder: repo,
            ..facts()
        };
        assert_eq!(smart_name(&named), "api");
        // A blank name is none.
        let blank = NameFacts {
            user: Some("  "),
            ..named
        };
        assert_eq!(smart_name(&blank), "claude · fuxx-cli");
        // 2. A program and its folder.
        let running = NameFacts {
            program: run("npm run dev"),
            folder: Some(Path::new("/Users/me/w/invitee")),
            ..facts()
        };
        assert_eq!(smart_name(&running), "npm run dev · invitee");
        let nowhere = NameFacts {
            program: run("htop"),
            ..facts()
        };
        assert_eq!(smart_name(&nowhere), "htop");
        // 3. At the prompt: the folder; home is `~`.
        let prompt = NameFacts {
            program: Prompt,
            folder: repo,
            title: Some("me@mac:~/Kerja/proj/fuxx-cli"),
            ..facts()
        };
        assert_eq!(smart_name(&prompt), "fuxx-cli");
        let home = NameFacts {
            program: Prompt,
            folder: Some(Path::new("/Users/me")),
            ..facts()
        };
        assert_eq!(smart_name(&home), "~");
        // 4. SSH: the host, then the host and the folder it reported.
        let ssh = NameFacts {
            ssh_host: Some("devbox"),
            preset: Some("claude"),
            ..facts()
        };
        assert_eq!(smart_name(&ssh), "devbox");
        let ssh_at = NameFacts {
            folder: Some(Path::new("/home/me/src")),
            ..ssh
        };
        assert_eq!(smart_name(&ssh_at), "devbox · src");
        // 5. Not looked up: the preset and the folder, else the folder.
        let preset = NameFacts {
            preset: Some("claude"),
            folder: repo,
            ..facts()
        };
        assert_eq!(smart_name(&preset), "claude · fuxx-cli");
        let unknown = NameFacts {
            folder: repo,
            title: Some("zsh"),
            ..facts()
        };
        assert_eq!(smart_name(&unknown), "fuxx-cli");
        // 6. Nothing known: the title, else ╭∩╮.
        let titled = NameFacts {
            title: Some("vim main.rs"),
            ..facts()
        };
        assert_eq!(smart_name(&titled), "vim main.rs");
        assert_eq!(smart_name(&facts()), groups::DEFAULT_NAME);
        let blank_title = NameFacts {
            title: Some(" "),
            ..facts()
        };
        assert_eq!(smart_name(&blank_title), groups::DEFAULT_NAME);
    }

    #[test]
    fn the_title_shows_in_a_tooltip_only_when_it_says_more() {
        assert_eq!(
            extra_title("fuxx-cli", Some("me@mac:~/fuxx-cli")),
            Some("me@mac:~/fuxx-cli")
        );
        assert_eq!(extra_title("fuxx-cli", Some("fuxx-cli")), None);
        assert_eq!(extra_title("fuxx-cli", Some("  ")), None);
        assert_eq!(extra_title("fuxx-cli", None), None);
    }

    #[test]
    fn a_new_program_shows_after_two_answers_in_a_row() {
        let mut s = Stable::new();
        assert!(s.offer(Prompt), "the first answer is taken");
        assert!(!s.offer(run("make")));
        assert_eq!(s.shown(), &Prompt);
        assert!(s.offer(run("make")));
        assert_eq!(s.shown(), &run("make"));
        // The same answer again changes nothing.
        assert!(!s.offer(run("make")));
    }

    #[test]
    fn a_known_agent_and_the_prompt_after_it_show_at_once() {
        let mut s = Stable::new();
        s.offer(Prompt);
        assert!(s.offer(run("claude")));
        assert_eq!(s.shown(), &run("claude"));
        assert!(s.offer(Prompt));
        assert_eq!(s.shown(), &Prompt);
    }

    #[test]
    fn alternating_answers_never_change_the_name() {
        let mut s = Stable::new();
        s.offer(Prompt);
        for _ in 0..4 {
            assert!(!s.offer(run("ls")));
            assert!(!s.offer(run("grep")));
        }
        assert_eq!(s.shown(), &Prompt);
        // Nor does a one-off answer between two of the same.
        assert!(!s.offer(run("make")));
        assert!(!s.offer(Prompt));
        assert!(!s.offer(run("make")));
        assert_eq!(s.shown(), &Prompt);
    }
}
