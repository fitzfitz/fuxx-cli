//! Whether an agent preset's program is on your `PATH` (M11k): the Welcome card's status words.
//! A guess from files only: nothing is run but the existing read of the login shell's
//! environment, and a wrong answer only changes a label.

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// What the check found for one preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    /// Not answered yet.
    Checking,
    Yes,
    No,
    /// The command can't be judged from files (a substitution, a relative path, no `PATH`).
    Unknown,
}

/// Only the first this many `PATH` folders are looked in.
pub const PATH_CAP: usize = 64;

/// The program a preset command starts: its first word after leading `NAME=value` words, one
/// layer of matching quotes removed. `None` (Unknown) for an empty command or one that starts
/// with `$`, `(` or a backtick, or with an unclosed quote.
pub fn program(command: &str) -> Option<String> {
    let mut rest = command.trim_start();
    loop {
        let (word, after) = next_word(rest)?;
        if is_assignment(word) {
            rest = after.trim_start();
            continue;
        }
        let word = unquote(word);
        if word.is_empty() || word.starts_with(['$', '(', '`']) {
            return None;
        }
        return Some(word.to_string());
    }
}

/// The first word of `s` (quotes may hold spaces) and what follows it; `None` if there is no
/// word or a quote is never closed.
fn next_word(s: &str) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c.is_whitespace() => {
                return (i > 0).then(|| (&s[..i], &s[i..]));
            }
            None => {}
        }
    }
    (quote.is_none() && !s.is_empty()).then_some((s, ""))
}

/// `NAME=value` with `NAME` of `[A-Za-z_][A-Za-z0-9_]*`.
fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// One layer of matching `'…'` or `"…"` removed.
fn unquote(word: &str) -> &str {
    for q in ['\'', '"'] {
        if let Some(inner) = word.strip_prefix(q).and_then(|w| w.strip_suffix(q)) {
            return inner;
        }
    }
    word
}

/// Whether `program` is an executable: a name is looked up in the first `PATH_CAP` non-empty
/// absolute folders of `path`; a program with a `/` is that path, if absolute.
pub fn on_path(program: &str, path: &OsStr, is_exec: impl Fn(&Path) -> bool) -> bool {
    if program.contains('/') {
        let p = Path::new(program);
        return p.is_absolute() && is_exec(p);
    }
    std::env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .take(PATH_CAP)
        .any(|dir| is_exec(&dir.join(program)))
}

/// A regular file with any execute bit.
pub fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The answer for each command (the pure core of `check`).
pub fn find(
    commands: &[String],
    path: Option<&OsStr>,
    is_exec: impl Fn(&Path) -> bool,
) -> Vec<Found> {
    commands
        .iter()
        .map(|command| match (path, program(command)) {
            (Some(path), Some(p)) if !p.contains('/') || Path::new(&p).is_absolute() => {
                if on_path(&p, path, &is_exec) {
                    Found::Yes
                } else {
                    Found::No
                }
            }
            _ => Found::Unknown,
        })
        .collect()
}

/// `find` against your login shell's `PATH` and the real files. Blocks (up to the login
/// environment's 3 s cap the first time): only from a background task.
pub fn check(commands: Vec<String>) -> Vec<Found> {
    let path = crate::workspace_ssh::login_env().and_then(|env| {
        env.iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| std::ffi::OsString::from(v))
    });
    find(&commands, path.as_deref(), is_executable_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("fuxx-agent-detect-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn make(path: &Path, mode: u32) {
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    fn s(v: &str) -> String {
        v.to_string()
    }

    #[test]
    fn program_takes_the_first_word() {
        assert_eq!(program("claude"), Some(s("claude")));
        assert_eq!(program("claude --continue \"fix: it\""), Some(s("claude")));
        assert_eq!(program("FOO=1 BAR=x aider --model s"), Some(s("aider")));
        assert_eq!(program("'/opt/my tools/a' --f"), Some(s("/opt/my tools/a")));
        assert_eq!(program("FOO=\"a b\" claude"), Some(s("claude")));
        for none in [
            "",
            "   ",
            "$(which x)",
            "(cd x; y)",
            "`x`",
            "FOO=1",
            "'unclosed x",
        ] {
            assert_eq!(program(none), None, "{none:?}");
        }
    }

    #[test]
    fn on_path_finds_an_executable() {
        let d = scratch("path");
        make(&d.join("claude"), 0o755);
        make(&d.join("codex"), 0o644);
        std::fs::create_dir(d.join("opencode")).unwrap();
        let path = OsString::from(format!("{}:relative::/nope", d.display()));
        assert!(on_path("claude", &path, is_executable_file));
        assert!(!on_path("codex", &path, is_executable_file));
        assert!(!on_path("opencode", &path, is_executable_file));
        assert!(!on_path("missing", &path, is_executable_file));

        let mut entries: Vec<String> = (0..65).map(|i| format!("/nope{i}")).collect();
        entries.push(d.display().to_string()); // the 66th
        entries.extend((0..3).map(|i| format!("/nope-after{i}")));
        let long = OsString::from(entries.join(":"));
        assert!(!on_path("claude", &long, is_executable_file));
        // At the 65th place it is just past the cap.
        let mut entries: Vec<String> = (0..64).map(|i| format!("/nope{i}")).collect();
        entries.push(d.display().to_string());
        assert!(!on_path(
            "claude",
            &OsString::from(entries.join(":")),
            is_executable_file
        ));
        // The same folder at the 64th place is still looked in.
        let mut entries: Vec<String> = (0..63).map(|i| format!("/nope{i}")).collect();
        entries.push(d.display().to_string());
        assert!(on_path(
            "claude",
            &OsString::from(entries.join(":")),
            is_executable_file
        ));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_program_with_a_slash_is_a_path() {
        let d = scratch("slash");
        make(&d.join("claude"), 0o755);
        let empty = OsString::new();
        assert!(on_path(
            &format!("{}/claude", d.display()),
            &empty,
            is_executable_file
        ));
        assert!(!on_path("bin/claude", &empty, |_| true));
        assert!(!on_path("./claude", &empty, |_| true));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn find_marks_each_command() {
        let d = scratch("find");
        make(&d.join("claude"), 0o755);
        let path = OsString::from(d.display().to_string());
        let commands = [s("claude"), s("codex"), s("$(x)")];
        assert_eq!(
            find(&commands, Some(&path), is_executable_file),
            vec![Found::Yes, Found::No, Found::Unknown]
        );
        assert_eq!(
            find(&[s("claude")], None, is_executable_file),
            vec![Found::Unknown]
        );
        // A relative path can't be judged; an absolute one that exists is found.
        let abs = format!("{}/claude --x", d.display());
        assert_eq!(
            find(&[s("bin/claude"), abs], Some(&path), is_executable_file),
            vec![Found::Unknown, Found::Yes]
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
