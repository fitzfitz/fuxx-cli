//! fuxx's zsh integration (M11l): the two scripts that make zsh send the command marks
//! (`shell_marks`), compiled in and written at startup to
//! `~/Library/Application Support/fuxx/shell-integration/zsh/`, and the environment that has a
//! login zsh read them first (`ZDOTDIR`, Ghostty's and Kitty's method). Your dotfiles are never
//! touched: fuxx's `.zshenv` puts your `ZDOTDIR` back and sources your own `.zshenv`, and zsh then
//! reads the rest of your files from your own folder as always. With a helper plugin on (M11z),
//! fuxx's `.zprofile` and `.zshrc` read yours with your `ZDOTDIR`, then load `plugins.zsh`.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The `.zshenv` zsh reads first when fuxx has set `ZDOTDIR` to its folder.
pub const ZSHENV: &str = include_str!("../assets/shell-integration/zsh/.zshenv");
/// The hooks, sourced by that `.zshenv` in an interactive shell.
pub const INTEGRATION: &str = include_str!("../assets/shell-integration/zsh/fuxx-integration.zsh");

/// With a helper plugin on (M11z): the `.zprofile` that reads yours with your `ZDOTDIR`.
pub const ZPROFILE: &str = include_str!("../assets/shell-integration/zsh/.zprofile");
/// With a helper plugin on: the `.zshrc` that puts your `ZDOTDIR` back, reads yours, then loads
/// `plugins.zsh` (written beside it by `plugin_shell`).
pub const ZSHRC: &str = include_str!("../assets/shell-integration/zsh/.zshrc");

/// Each script, by its file name in `<dir>/zsh/`.
const FILES: [(&str, &str); 4] = [
    (".zshenv", ZSHENV),
    ("fuxx-integration.zsh", INTEGRATION),
    (".zprofile", ZPROFILE),
    (".zshrc", ZSHRC),
];

/// Set once by `setup` when the scripts are in place.
static INSTALLED: OnceLock<PathBuf> = OnceLock::new();

/// `$HOME/Library/Application Support/fuxx/shell-integration`; none without `$HOME`.
pub fn integration_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join("Library/Application Support/fuxx/shell-integration"))
}

/// Write the scripts into `dir/zsh/` (`0o644`), each only when missing or different. `Ok(true)`
/// when something was written. Each file is written beside its place and renamed over it, so a
/// zsh starting meanwhile reads the old script or the new one, never half of one.
pub fn install(dir: &Path) -> io::Result<bool> {
    let zsh = dir.join("zsh");
    fs::create_dir_all(&zsh)?;
    let mut wrote = false;
    for (name, text) in FILES {
        let path = zsh.join(name);
        if fs::read(&path).is_ok_and(|now| now == text.as_bytes()) {
            continue;
        }
        let tmp = zsh.join(format!("{name}.{}.tmp", std::process::id()));
        let written = write_file(&tmp, text).and_then(|()| fs::rename(&tmp, &path));
        if written.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        written?;
        wrote = true;
    }
    Ok(wrote)
}

/// `text` as a new file at `path`, `0o644` whatever the umask.
fn write_file(path: &Path, text: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.set_permissions(fs::Permissions::from_mode(0o644))
}

/// At startup, before any terminal: install the scripts in `integration_dir`. On failure, one
/// log line, and no terminal gets the integration this run (its zsh starts as without fuxx).
pub fn setup() {
    let Some(dir) = integration_dir() else {
        eprintln!("[fuxx] shell integration: $HOME is unset; zsh starts without it");
        return;
    };
    match install(&dir) {
        Ok(_) => {
            let _ = INSTALLED.set(dir);
        }
        Err(err) => eprintln!(
            "[fuxx] shell integration: could not write {}: {err}; zsh starts without it",
            dir.display()
        ),
    }
}

/// Whether the scripts in `dir` (`installed`) can be given to a new shell, checked at each
/// spawn: each is there (a stat), or, if one was deleted while fuxx runs, installing them again
/// worked. Otherwise `ZDOTDIR` would point at a missing folder, or (with a helper plugin on) at
/// one without fuxx's `.zshrc`, and zsh would start without your own files.
pub fn usable(dir: &Path) -> bool {
    let zsh = dir.join("zsh");
    if FILES.iter().all(|(name, _)| zsh.join(name).is_file()) {
        return true;
    }
    match install(dir) {
        Ok(_) => true,
        Err(err) => {
            eprintln!(
                "[fuxx] shell integration: {} is gone and could not be written again: {err}; \
                 zsh starts without it",
                dir.display()
            );
            false
        }
    }
}

/// Where the scripts were installed at startup; none before, or if that failed.
pub fn installed() -> Option<&'static Path> {
    INSTALLED.get().map(PathBuf::as_path)
}

/// What a new shell's environment gets so zsh reads the integration: `ZDOTDIR=<dir>/zsh`,
/// `FUXX_ZSH_ZDOTDIR` with yours when fuxx's own environment has one, `FUXX_ZSH_MARKS=1` with
/// `marks` (`command-blocks`) and `FUXX_ZSH_PLUGINS=1` with `plugins` (a helper plugin that runs
/// code is on, M11z). Nothing unless `shell`'s file name is exactly `zsh`, the scripts are
/// installed (`dir`) and `marks` or `plugins`.
pub fn env_for(
    shell: &Path,
    user_zdotdir: Option<&OsStr>,
    marks: bool,
    plugins: bool,
    dir: Option<&Path>,
) -> Vec<(String, String)> {
    let Some(dir) = dir.filter(|_| marks || plugins) else {
        return Vec::new();
    };
    if shell.file_name() != Some(OsStr::new("zsh")) {
        return Vec::new();
    }
    let mut env = vec![("ZDOTDIR".to_string(), dir.join("zsh").display().to_string())];
    if let Some(yours) = user_zdotdir {
        env.push((
            "FUXX_ZSH_ZDOTDIR".to_string(),
            yours.to_string_lossy().into_owned(),
        ));
    }
    for (on, key) in [(marks, "FUXX_ZSH_MARKS"), (plugins, "FUXX_ZSH_PLUGINS")] {
        if on {
            env.push((key.to_string(), "1".to_string()));
        }
    }
    env
}

/// For your login shell: add to `env` what `env_for` gives for `shell` (fuxx's `$SHELL`) with
/// `user_zdotdir` (fuxx's own `ZDOTDIR`), `marks`, `plugins` and `integration` (the installed
/// folder). When it injects and you have no `ZDOTDIR`, a `FUXX_ZSH_ZDOTDIR` inherited from
/// elsewhere is removed, or fuxx's `.zshenv` would set `ZDOTDIR` to it.
pub fn add_login_shell_env(
    env: &mut crate::pty::ShellEnv,
    shell: &OsStr,
    user_zdotdir: Option<&OsStr>,
    marks: bool,
    plugins: bool,
    integration: Option<&Path>,
) {
    let added = env_for(Path::new(shell), user_zdotdir, marks, plugins, integration);
    if added.is_empty() {
        return;
    }
    if user_zdotdir.is_none() {
        env.remove.push("FUXX_ZSH_ZDOTDIR");
    }
    env.set.extend(added);
}

/// A real zsh in a terminal, for the tests here and elsewhere (M11z's end-to-end plugin test).
#[cfg(test)]
pub(crate) mod test_zsh {
    use super::env_for;
    use crate::blocks::{Block, Outcome};
    use crate::terminal::{TermDims, TermEvent, TerminalCore};
    use crate::theme::Theme;
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};
    use portable_pty::CommandBuilder;
    use std::path::Path;
    use std::time::{Duration, Instant};

    /// Poll `f` until it holds or 10 s pass.
    pub(crate) fn until(mut f: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// `/bin/zsh -il` in a terminal: `spawn_as` with `-il`.
    pub(crate) fn spawn(
        home: &Path,
        user_zdotdir: Option<&Path>,
        dir: &Path,
        marks: bool,
        plugins: bool,
    ) -> (TerminalCore, UnboundedReceiver<TermEvent>) {
        spawn_as("-il", home, user_zdotdir, dir, marks, plugins)
    }

    /// `/bin/zsh <flags>` in a terminal, `HOME` = `home`, with only the environment a fuxx
    /// terminal would add (`env_for` with `marks` and `plugins`, the scripts in `dir`) on a small
    /// fixed one, and `ZDOTDIR` = `user_zdotdir` if given.
    pub(crate) fn spawn_as(
        flags: &str,
        home: &Path,
        user_zdotdir: Option<&Path>,
        dir: &Path,
        marks: bool,
        plugins: bool,
    ) -> (TerminalCore, UnboundedReceiver<TermEvent>) {
        let zd = user_zdotdir.map(Path::as_os_str);
        let added = env_for(Path::new("/bin/zsh"), zd, marks, plugins, Some(dir));
        spawn_with(flags, home, user_zdotdir, &added)
    }

    /// `/bin/zsh <flags>` in a terminal as `spawn_as` starts it, with `added` in place of what
    /// `env_for` gives.
    pub(crate) fn spawn_with(
        flags: &str,
        home: &Path,
        user_zdotdir: Option<&Path>,
        added: &[(String, String)],
    ) -> (TerminalCore, UnboundedReceiver<TermEvent>) {
        let mut cmd = CommandBuilder::new("/bin/zsh");
        cmd.arg(flags);
        cmd.env_clear();
        cmd.env("HOME", home);
        cmd.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        cmd.env("TERM", "xterm-256color");
        cmd.env("LANG", "en_US.UTF-8");
        if let Some(zd) = user_zdotdir {
            cmd.env("ZDOTDIR", zd);
        }
        for (key, value) in added {
            cmd.env(key, value);
        }
        cmd.cwd(home);
        let (tx, rx) = unbounded();
        let core = TerminalCore::spawn(cmd, TermDims { cols: 80, rows: 30 }, tx, Theme::default())
            .expect("spawn zsh");
        (core, rx)
    }

    pub(crate) fn screen_text(core: &TerminalCore) -> String {
        core.snapshot()
            .lines
            .iter()
            .map(|l| l.iter().map(|c| c.ch).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Type `line` and Return at the ready prompt, then wait for its block to finish and the
    /// next prompt to be ready. The blocks then. Needs the marks (`command-blocks` on).
    pub(crate) fn run(core: &TerminalCore, line: &str) -> Vec<Block> {
        assert!(
            until(|| core.prompt_ready() && core.cursor_at_input_start()),
            "no ready prompt before {line:?}; screen:\n{}",
            screen_text(core)
        );
        let before = core.blocks().len();
        core.input(format!("{line}\r").as_bytes());
        let done = until(|| {
            let blocks = core.blocks();
            blocks.len() == before + 1
                && blocks.last().is_some_and(|b| b.outcome != Outcome::Running)
                && core.prompt_ready()
        });
        assert!(
            done,
            "{line:?} didn't finish as a block; blocks {:?}; screen:\n{}",
            core.blocks(),
            screen_text(core)
        );
        core.blocks()
    }
}

#[cfg(test)]
mod tests {
    use super::test_zsh::{self, run, screen_text, until};
    use super::*;
    use crate::blocks::{Block, Outcome};
    use crate::shell_marks::percent_decode;
    use crate::terminal::{TermEvent, TerminalCore};
    use futures::channel::mpsc::UnboundedReceiver;
    use std::time::Duration;

    fn kv(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn env_for_zsh_only() {
        let i = Some(Path::new("/i"));
        let zsh = Path::new("/bin/zsh");
        let marks = ("FUXX_ZSH_MARKS", "1");
        assert_eq!(
            env_for(zsh, None, true, false, i),
            kv(&[("ZDOTDIR", "/i/zsh"), marks])
        );
        assert_eq!(
            env_for(zsh, Some(OsStr::new("/u/zd")), true, false, i),
            kv(&[("ZDOTDIR", "/i/zsh"), ("FUXX_ZSH_ZDOTDIR", "/u/zd"), marks])
        );
        assert_eq!(
            env_for(Path::new("/opt/homebrew/bin/zsh"), None, true, false, i),
            kv(&[("ZDOTDIR", "/i/zsh"), marks])
        );
        for other in ["/bin/bash", "/usr/local/bin/fish", "/bin/sh", "/bin/zsh5"] {
            assert_eq!(
                env_for(Path::new(other), None, true, true, i),
                [],
                "{other}"
            );
        }
        assert_eq!(env_for(zsh, Some(OsStr::new("/u/zd")), false, false, i), []);
        assert_eq!(
            env_for(zsh, Some(OsStr::new("/u/zd")), true, true, None),
            []
        );
    }

    #[test]
    fn env_for_flags() {
        let i = Some(Path::new("/i"));
        let zsh = Path::new("/bin/zsh");
        let zd = Some(OsStr::new("/u/zd"));
        let base = [("ZDOTDIR", "/i/zsh"), ("FUXX_ZSH_ZDOTDIR", "/u/zd")];
        let marks = ("FUXX_ZSH_MARKS", "1");
        let plugins = ("FUXX_ZSH_PLUGINS", "1");
        assert_eq!(env_for(zsh, zd, false, false, i), []);
        assert_eq!(
            env_for(zsh, zd, true, false, i),
            kv(&[base[0], base[1], marks])
        );
        assert_eq!(
            env_for(zsh, zd, false, true, i),
            kv(&[base[0], base[1], plugins])
        );
        assert_eq!(
            env_for(zsh, zd, true, true, i),
            kv(&[base[0], base[1], marks, plugins])
        );
        assert_eq!(env_for(zsh, None, false, true, i), kv(&[base[0], plugins]));
    }

    /// A fresh, empty folder under the system's temp folder, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("fuxx-si-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn write(&self, rel: &str, text: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn install_writes_once() {
        use std::os::unix::fs::PermissionsExt;
        let t = TempDir::new("install");
        let dir = t.0.join("shell-integration");
        assert!(install(&dir).unwrap());
        let read = |name: &str| std::fs::read_to_string(dir.join("zsh").join(name)).unwrap();
        assert_eq!(read(".zshenv"), ZSHENV);
        assert_eq!(read("fuxx-integration.zsh"), INTEGRATION);
        assert_eq!(read(".zprofile"), ZPROFILE);
        assert_eq!(read(".zshrc"), ZSHRC);
        for name in [".zshenv", "fuxx-integration.zsh", ".zprofile", ".zshrc"] {
            let mode = std::fs::metadata(dir.join("zsh").join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o644, "{name}");
        }
        // The same again: nothing written.
        assert!(!install(&dir).unwrap());
        // Changed by hand: put back.
        std::fs::write(dir.join("zsh/fuxx-integration.zsh"), "echo changed\n").unwrap();
        assert!(install(&dir).unwrap());
        assert_eq!(read("fuxx-integration.zsh"), INTEGRATION);
        assert_eq!(read(".zshenv"), ZSHENV);
        // Nothing else is left in the folder (no temporary files).
        let mut names: Vec<String> = std::fs::read_dir(dir.join("zsh"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [".zprofile", ".zshenv", ".zshrc", "fuxx-integration.zsh"]
        );
    }

    #[test]
    fn usable_checks_the_scripts_at_each_spawn() {
        let t = TempDir::new("usable");
        let dir = t.0.join("shell-integration");
        install(&dir).unwrap();
        assert!(usable(&dir));
        // Deleted while fuxx runs: installed again, once.
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(usable(&dir));
        assert_eq!(
            std::fs::read_to_string(dir.join("zsh/.zshenv")).unwrap(),
            ZSHENV
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("zsh/fuxx-integration.zsh")).unwrap(),
            INTEGRATION
        );
        // Only fuxx's .zshrc gone: in plugin mode zsh would skip yours, so it is written again.
        std::fs::remove_file(dir.join("zsh/.zshrc")).unwrap();
        assert!(usable(&dir));
        assert_eq!(
            std::fs::read_to_string(dir.join("zsh/.zshrc")).unwrap(),
            ZSHRC
        );
        // Gone and can't be written again (its parent is a file): not usable, no ZDOTDIR.
        let file = t.write("a-file", "x");
        assert!(!usable(&file.join("shell-integration")));
    }

    #[test]
    fn integration_dir_is_under_application_support() {
        let home = std::env::var_os("HOME").filter(|h| !h.is_empty());
        assert_eq!(
            integration_dir(),
            home.map(
                |h| PathBuf::from(h).join("Library/Application Support/fuxx/shell-integration")
            )
        );
    }

    /// Run `script` in `zsh -f` with the integration sourced (no terminal); its output.
    fn zsh_f(t: &TempDir, script: &str) -> Vec<u8> {
        let dir = t.0.join("si");
        install(&dir).unwrap();
        let integration = dir.join("zsh/fuxx-integration.zsh");
        let mut cmd = std::process::Command::new("/bin/zsh");
        // A new session, without a controlling terminal: no /dev/tty, so the marks come on
        // stdout, where they are read here.
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                setsid();
                Ok(())
            });
        }
        let out = cmd
            .args(["-f", "-c"])
            .arg(format!(
                "self={}; source \"$self\"\n{script}",
                shell_quote(&integration.display().to_string())
            ))
            .env("LANG", "en_US.UTF-8")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "zsh failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out.stdout
    }

    unsafe extern "C" {
        fn setsid() -> i32;
    }

    fn shell_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', r"'\''"))
    }

    /// The `cmdline_url` of each C mark in `out` (`None` for a C without one), and the other
    /// marks as their letter and arguments.
    fn marks_in(out: &[u8]) -> Vec<String> {
        let text = String::from_utf8_lossy(out);
        text.split("\x1b]133;")
            .skip(1)
            .map(|m| m.split('\x07').next().unwrap_or("").to_string())
            .collect()
    }

    #[test]
    fn preexec_sends_the_command_percent_encoded() {
        let t = TempDir::new("encode");
        let commands = [
            "echo hi",
            "ls -la ~/Code\\ Projects | grep 'a;b' && echo \"100%\"",
            "echo héllo 日本 🦀",
            "printf 'a\\tb'\necho second line",
            "echo $'\\e[31m' \\\\ %41 + # ? & = [x] {y}",
        ];
        for cmd in commands {
            let out = zsh_f(&t, &format!("_fuxx_preexec {} x x", shell_quote(cmd)));
            let marks = marks_in(&out);
            assert_eq!(marks.len(), 1, "{cmd:?}: {marks:?}");
            let url = marks[0]
                .strip_prefix("C;cmdline_url=")
                .unwrap_or_else(|| panic!("{cmd:?}: {:?}", marks[0]));
            // Only printable ASCII, never `;`: the tag carries it as it is.
            assert!(
                url.bytes().all(|b| (0x21..=0x7e).contains(&b) && b != b';'),
                "{cmd:?}: {url}"
            );
            assert_eq!(percent_decode(url.as_bytes()).as_deref(), Some(cmd));
        }
        // 2,048 bytes of UTF-8 are sent; one more and the command is left out.
        let at_cap = format!("echo {}", "é".repeat((2048 - 5) / 2)) + "x";
        assert_eq!(at_cap.len(), 2048);
        let marks = marks_in(&zsh_f(
            &t,
            &format!("_fuxx_preexec {}", shell_quote(&at_cap)),
        ));
        let url = marks[0].strip_prefix("C;cmdline_url=").expect("sent");
        assert_eq!(percent_decode(url.as_bytes()), Some(at_cap.clone()));
        let over = at_cap + "y";
        let marks = marks_in(&zsh_f(&t, &format!("_fuxx_preexec {}", shell_quote(&over))));
        assert_eq!(marks, ["C"]);
    }

    #[test]
    fn precmd_sends_done_only_after_a_command_ran() {
        let t = TempDir::new("done");
        // No command since the last prompt: no D.
        let marks = marks_in(&zsh_f(&t, "false; _fuxx_done; _fuxx_precmd"));
        assert_eq!(marks, Vec::<String>::new());
        // A command ran: D with its status, once, from `_fuxx_done` (the prompt hook sends none).
        let marks = marks_in(&zsh_f(
            &t,
            "_fuxx_preexec 'false'; (exit 3); _fuxx_done; _fuxx_precmd; true; _fuxx_done",
        ));
        assert_eq!(marks, ["C;cmdline_url=false", "D;3"]);
    }

    #[test]
    fn preexec_sends_no_c_for_an_exec_that_replaces_the_shell() {
        // The new shell never marks (fuxx's ZDOTDIR is gone by then), so a C would leave a
        // block running for good: an `exec` that replaces the shell is not a block.
        let t = TempDir::new("exec");
        let show = "; print -r -- \"[$_fuxx_running]\"";
        for line in [
            "exec zsh",
            "exec -l zsh",
            "builtin exec zsh",
            "command exec zsh",
            "builtin command exec /bin/zsh -l",
            "  exec   zsh",
            // Parameter assignments before it (for the new shell's environment).
            "FOO=1 exec zsh",
            "FOO=1 BAR_2=x exec -l zsh",
            "FOO='a b' builtin exec zsh",
        ] {
            let out = zsh_f(&t, &format!("_fuxx_preexec {}{show}", shell_quote(line)));
            assert_eq!(String::from_utf8_lossy(&out), "[0]\n", "{line:?}");
        }
        // Only redirections (the shell stays), a bare exec, or exec not the command: a block.
        for line in [
            "exec >out.txt",
            "exec 2>&1",
            "exec {fd}>f",
            "exec &>f",
            "exec <in",
            "exec >&-",
            "exec",
            "echo exec zsh",
            "execute zsh",
            "FOO=1 ls",
            "FOO=1 exec >out.txt",
            "FOO=1",
            "echo x; exec zsh",
        ] {
            let out = zsh_f(&t, &format!("_fuxx_preexec {}{show}", shell_quote(line)));
            let text = String::from_utf8_lossy(&out);
            assert!(
                text.starts_with("\x1b]133;C;cmdline_url=") && text.ends_with("\x07[1]\n"),
                "{line:?}: {text:?}"
            );
        }
    }

    #[test]
    fn done_is_first_in_precmd_and_stays_first() {
        // D goes before every other precmd hook, so what they print (a timing plugin) is not
        // the command's output; the prompt's hook stays last.
        let t = TempDir::new("done-first");
        let out = zsh_f(
            &t,
            "PS1='%# '; print -r -- \"[$precmd_functions]\"\n\
             front() { :; }; back() { :; }; precmd_functions=(front $precmd_functions back)\n\
             _fuxx_precmd; print -r -- \"[$precmd_functions]\"\n\
             precmd_functions=(front ${precmd_functions:#front})\n\
             _fuxx_line_init; print -r -- \"[$precmd_functions]\"\n\
             typeset -gi _POWERLEVEL9K_TERM_SHELL_INTEGRATION=1\n\
             precmd_functions=(front ${precmd_functions:#front})\n\
             _fuxx_line_init; print -r -- \"[$precmd_functions]\"",
        );
        let text = String::from_utf8_lossy(&out);
        assert_eq!(
            text.lines().collect::<Vec<_>>(),
            [
                "[_fuxx_done _fuxx_precmd]",
                "[_fuxx_done front back _fuxx_precmd]",
                "[_fuxx_done front back _fuxx_precmd]",
                // powerlevel10k marking its prompts: D still put first.
                "[_fuxx_done front back _fuxx_precmd]",
            ],
            "{text}"
        );
    }

    #[test]
    fn precmd_marks_the_prompt_once_and_moves_last() {
        let t = TempDir::new("prompt");
        let out = zsh_f(
            &t,
            "PS1='%# '; theme() { :; }; precmd_functions+=(theme)\n\
             _fuxx_precmd; print -r -- \"[${(V)PS1}]\"\n\
             _fuxx_precmd; print -r -- \"[${(V)PS1}]\"\n\
             print -r -- \"[$precmd_functions]\"\n\
             PS1='end%'; _fuxx_precmd; print -r -- \"[${(V)PS1}]\"\n\
             source \"$self\"; print -r -- \"[$precmd_functions][$preexec_functions]\"\n\
             a=$'%{\\e]133;A\\a%}' b=$'%{\\e]133;B\\a%}'\n\
             PS1=\"pre\"$'\\n'\"$a%# $b\"; _fuxx_precmd; print -r -- \"[${(V)PS1}]\"\n\
             PS1=\"$a$b> x$b\"; _fuxx_precmd; print -r -- \"[${(V)PS1}]\"",
        );
        let text = String::from_utf8_lossy(&out);
        let lines: Vec<&str> = text.lines().collect();
        let marked = "[%{^[]133;A^G%}%# %{^[]133;B^G%}]";
        assert_eq!(lines[0], marked);
        // Already marked: unchanged.
        assert_eq!(lines[1], marked);
        assert_eq!(lines[2], "[_fuxx_done theme _fuxx_precmd]");
        // A lone % at the end is doubled, so it can't eat the B mark's `%{`.
        assert_eq!(lines[3], "[%{^[]133;A^G%}end%%%{^[]133;B^G%}]");
        // Sourced again: nothing added twice.
        assert_eq!(lines[4], "[_fuxx_done theme _fuxx_precmd][_fuxx_preexec]");
        // Copies kept inside by a theme are dropped: one A at the start, one B at the end.
        assert_eq!(lines[5], "[%{^[]133;A^G%}pre\\n%# %{^[]133;B^G%}]");
        assert_eq!(lines[6], "[%{^[]133;A^G%}> x%{^[]133;B^G%}]");
    }

    #[test]
    fn precmd_moves_last_at_every_prompt() {
        // A theme that moves its own hook last before every prompt (powerlevel10k). At the
        // first prompt it runs after fuxx's anyway (the hooks were listed before either
        // moved), and the line-init hook, once the prompt is up, puts fuxx's last again. From
        // then on fuxx's hook runs last and stays last after each prompt.
        let t = TempDir::new("order");
        let out = zsh_f(
            &t,
            "PS1='%# '; theme() { precmd_functions=(${precmd_functions:#theme} theme) }\n\
             precmd_functions+=(theme)\n\
             for i in 1 2 3; do\n\
               for f in $precmd_functions; do $f; done\n\
               print -rn -- \"[$precmd_functions]\"\n\
               _fuxx_line_init; print -r -- \"[$precmd_functions]\"\n\
             done",
        );
        let text = String::from_utf8_lossy(&out);
        assert_eq!(
            text.lines().collect::<Vec<_>>(),
            [
                "[_fuxx_done _fuxx_precmd theme][_fuxx_done theme _fuxx_precmd]",
                "[_fuxx_done theme _fuxx_precmd][_fuxx_done theme _fuxx_precmd]",
                "[_fuxx_done theme _fuxx_precmd][_fuxx_done theme _fuxx_precmd]"
            ],
            "{text}"
        );
    }

    #[test]
    fn precmd_turns_on_p10k_marks_only_when_unset() {
        let t = TempDir::new("p10k-param");
        let show = "print -r -- \"[${POWERLEVEL9K_TERM_SHELL_INTEGRATION-unset}]\"";
        // p10k loaded, the setting unset: turned on.
        let out = zsh_f(&t, &format!("p10k() {{ :; }}; _fuxx_precmd; {show}"));
        assert_eq!(String::from_utf8_lossy(&out), "[true]\n");
        // Set by you (even to false): kept.
        let out = zsh_f(
            &t,
            &format!(
                "p10k() {{ :; }}; POWERLEVEL9K_TERM_SHELL_INTEGRATION=false; _fuxx_precmd; {show}"
            ),
        );
        assert_eq!(String::from_utf8_lossy(&out), "[false]\n");
        // No p10k: not set.
        let out = zsh_f(&t, &format!("_fuxx_precmd; {show}"));
        assert_eq!(String::from_utf8_lossy(&out), "[unset]\n");
    }

    #[test]
    fn p10k_marking_itself_leaves_ps1_and_turns_its_c_and_d_off() {
        let t = TempDir::new("p10k-native");
        // p10k reports that it marks its prompts: PS1 is left alone, its own C and D are
        // turned off (`_p9k__iterm_cmd` unset), and fuxx's D still comes after a command.
        let out = zsh_f(
            &t,
            "exec 2>&1; p10k() { :; }\n\
             typeset -gi _POWERLEVEL9K_TERM_SHELL_INTEGRATION=1 _p9k__iterm_cmd=1\n\
             PS1='%# '; _fuxx_preexec 'false'; (exit 1); _fuxx_done; _fuxx_precmd\n\
             print -r -- \"[${(V)PS1}][${_p9k__iterm_cmd-unset}]\"",
        );
        assert_eq!(
            String::from_utf8_lossy(&out),
            "\x1b]133;C;cmdline_url=false\x07\x1b]133;D;1\x07[%# ][unset]\n"
        );
        // Without those internals (another p10k version), or with its marks off: fuxx marks
        // PS1 itself, and nothing errs.
        for setup in [
            "p10k() { :; }",
            "p10k() { :; }; typeset -gi _POWERLEVEL9K_TERM_SHELL_INTEGRATION=0 _p9k__iterm_cmd=1",
            "p10k() { :; }; typeset -gi _POWERLEVEL9K_TERM_SHELL_INTEGRATION=1",
        ] {
            let out = zsh_f(
                &t,
                &format!(
                    "exec 2>&1; {setup}\nPS1='%# '; _fuxx_precmd; _fuxx_precmd\n\
                     print -r -- \"[${{(V)PS1}}]\""
                ),
            );
            let text = String::from_utf8_lossy(&out);
            let expect = if setup.ends_with("INTEGRATION=1") {
                "[%# ]\n"
            } else {
                "[%{^[]133;A^G%}%# %{^[]133;B^G%}]\n"
            };
            assert_eq!(text, expect, "{setup}");
        }
    }

    // --- A real zsh in a terminal ---

    /// `/bin/zsh -il` as a fuxx terminal starts it while `command-blocks` is on and no helper
    /// plugin is (M11l).
    fn spawn_zsh(
        home: &Path,
        user_zdotdir: Option<&Path>,
        dir: &Path,
    ) -> (TerminalCore, UnboundedReceiver<TermEvent>) {
        test_zsh::spawn(home, user_zdotdir, dir, true, false)
    }

    fn outcomes(blocks: &[Block]) -> Vec<Outcome> {
        blocks.iter().map(|b| b.outcome).collect()
    }

    fn cmds(blocks: &[Block]) -> Vec<&str> {
        blocks
            .iter()
            .map(|b| b.cmd.as_deref().unwrap_or("?"))
            .collect()
    }

    /// Each startup file adds its name to `T_LOG`, so running twice or out of order shows.
    const LOG: &str = "T_LOG=\"${T_LOG}";

    const CHECK: &str = "print -r -- \"[$ZDOTDIR][$T_ENV][$T_RC][${FUXX_ZSH_ZDOTDIR-unset}]\"";
    const ORDER_AND_CHILD: &str =
        "print -r -- $T_LOG; /bin/sh -c 'echo \"[${ZDOTDIR-unset}][${FUXX_ZSH_ZDOTDIR-unset}]\"'";

    fn startup_files(t: &TempDir, folder: &str) {
        let at = |name: &str| format!("{folder}/{name}");
        t.write(&at(".zshenv"), &format!("export T_ENV=1\n{LOG}env\"\n"));
        t.write(&at(".zprofile"), &format!("{LOG}:profile\"\n"));
        t.write(
            &at(".zshrc"),
            &format!("PS1='%# '; export T_RC=1\n{LOG}:rc\"\n"),
        );
        t.write(&at(".zlogin"), &format!("{LOG}:login\"\n"));
    }

    #[test]
    fn real_zsh_keeps_your_zdotdir_and_files() {
        let t = TempDir::new("real");
        let dir = t.0.join("si");
        install(&dir).unwrap();

        // No ZDOTDIR of yours: your files in HOME, and ZDOTDIR unset inside.
        let home = t.0.join("home");
        startup_files(&t, "home");
        let (core, _rx) = spawn_zsh(&home, None, &dir);
        run(&core, "echo hi");
        run(&core, "false");
        let blocks = run(&core, CHECK);
        assert_eq!(
            outcomes(&blocks),
            [Outcome::Ok, Outcome::Failed(1), Outcome::Ok]
        );
        assert_eq!(cmds(&blocks)[..2], ["echo hi", "false"]);
        assert_eq!(cmds(&blocks)[2], CHECK);
        assert_eq!(
            core.block_output(&blocks[2].id).as_deref(),
            Some("[][1][1][unset]")
        );
        let blocks = run(&core, ORDER_AND_CHILD);
        assert_eq!(
            core.block_output(&blocks[3].id).as_deref(),
            Some("env:profile:rc:login\n[unset][unset]")
        );
        drop(core);

        // Your own ZDOTDIR: your files there, and ZDOTDIR yours inside and in children.
        let home2 = t.0.join("home2");
        std::fs::create_dir_all(&home2).unwrap();
        startup_files(&t, "zd");
        let zd = t.0.join("zd");
        let (core, _rx) = spawn_zsh(&home2, Some(&zd), &dir);
        let blocks = run(&core, CHECK);
        assert_eq!(outcomes(&blocks), [Outcome::Ok]);
        assert_eq!(
            core.block_output(&blocks[0].id),
            Some(format!("[{}][1][1][unset]", zd.display()))
        );
        let blocks = run(&core, ORDER_AND_CHILD);
        assert_eq!(
            core.block_output(&blocks[1].id),
            Some(format!("env:profile:rc:login\n[{}][unset]", zd.display()))
        );
        drop(core);

        // Your .zshenv sets ZDOTDIR itself: the rest is read from that folder, not HOME.
        let home3 = t.0.join("home3");
        let zd3 = home3.join("zd3");
        t.write(
            "home3/.zshenv",
            &format!("export T_ENV=1 ZDOTDIR=\"$HOME/zd3\"\n{LOG}env\"\n"),
        );
        for name in [".zprofile", ".zshrc", ".zlogin"] {
            t.write(&format!("home3/{name}"), &format!("{LOG}:wrong-{name}\"\n"));
        }
        t.write("home3/zd3/.zprofile", &format!("{LOG}:profile\"\n"));
        t.write(
            "home3/zd3/.zshrc",
            &format!("PS1='%# '; export T_RC=1\n{LOG}:rc\"\n"),
        );
        t.write("home3/zd3/.zlogin", &format!("{LOG}:login\"\n"));
        let (core, _rx) = spawn_zsh(&home3, None, &dir);
        let blocks = run(&core, CHECK);
        assert_eq!(
            core.block_output(&blocks[0].id),
            Some(format!("[{}][1][1][unset]", zd3.display()))
        );
        let blocks = run(&core, ORDER_AND_CHILD);
        assert_eq!(
            core.block_output(&blocks[1].id),
            Some(format!("env:profile:rc:login\n[{}][unset]", zd3.display()))
        );
    }

    // --- Plugin mode (M11z): fuxx's .zprofile and .zshrc around yours ---

    /// The last block's output.
    fn last_output(core: &TerminalCore, blocks: &[Block]) -> Option<String> {
        core.block_output(&blocks.last().unwrap().id)
    }

    /// What `HISTFILE` is without fuxx: macOS's `/etc/zshrc` sets it from your `ZDOTDIR` (or
    /// `HOME`), read while fuxx's folder is `ZDOTDIR` in plugin mode.
    fn histfile(yours: &Path) -> String {
        let etc = std::fs::read_to_string("/etc/zshrc").unwrap_or_default();
        if etc.contains("HISTFILE=${ZDOTDIR:-$HOME}/.zsh_history") {
            format!("{}/.zsh_history", yours.display())
        } else {
            String::new()
        }
    }

    /// fuxx's helpers left in the shell, `HISTFILE`, and the flags a child would get.
    const CLEAN: &str = "print -r -- \"[${(k)parameters[(I)(_fuxx_zdotdir*|_fuxx_file)]}][$HISTFILE][$T_PM]\"; \
         /bin/sh -c 'echo \"[${FUXX_ZSH_MARKS-unset}][${FUXX_ZSH_PLUGINS-unset}]\"'";

    #[test]
    fn plugin_mode_runs_your_files_once_in_order() {
        let t = TempDir::new("plugin-order");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        // plugins.zsh only marks that it was read (`T_PM`), and adds nothing to the log.
        t.write("si/zsh/plugins.zsh", "# fuxx plugins\nT_PM=1\n");
        let clean = |core: &TerminalCore, yours: &Path| {
            let blocks = run(core, CLEAN);
            assert_eq!(
                last_output(core, &blocks),
                Some(format!("[][{}][1]\n[unset][unset]", histfile(yours)))
            );
        };

        // No ZDOTDIR of yours: your files in HOME, and ZDOTDIR unset inside.
        let home = t.0.join("home");
        startup_files(&t, "home");
        let (core, _rx) = test_zsh::spawn(&home, None, &dir, true, true);
        let blocks = run(&core, CHECK);
        assert_eq!(
            last_output(&core, &blocks).as_deref(),
            Some("[][1][1][unset]")
        );
        let blocks = run(&core, ORDER_AND_CHILD);
        assert_eq!(
            last_output(&core, &blocks).as_deref(),
            Some("env:profile:rc:login\n[unset][unset]")
        );
        clean(&core, &home);
        drop(core);

        // Your own ZDOTDIR: your files there, and ZDOTDIR yours inside and in children.
        let home2 = t.0.join("home2");
        std::fs::create_dir_all(&home2).unwrap();
        startup_files(&t, "zd");
        let zd = t.0.join("zd");
        let (core, _rx) = test_zsh::spawn(&home2, Some(&zd), &dir, true, true);
        let blocks = run(&core, CHECK);
        assert_eq!(
            last_output(&core, &blocks),
            Some(format!("[{}][1][1][unset]", zd.display()))
        );
        let blocks = run(&core, ORDER_AND_CHILD);
        assert_eq!(
            last_output(&core, &blocks),
            Some(format!("env:profile:rc:login\n[{}][unset]", zd.display()))
        );
        clean(&core, &zd);
        drop(core);

        // Your .zshenv sets ZDOTDIR itself: the rest is read from that folder, not HOME.
        let home3 = t.0.join("home3");
        let zd3 = home3.join("zd3");
        t.write(
            "home3/.zshenv",
            &format!("export T_ENV=1 ZDOTDIR=\"$HOME/zd3\"\n{LOG}env\"\n"),
        );
        for name in [".zprofile", ".zshrc", ".zlogin"] {
            t.write(&format!("home3/{name}"), &format!("{LOG}:wrong-{name}\"\n"));
        }
        t.write("home3/zd3/.zprofile", &format!("{LOG}:profile\"\n"));
        t.write(
            "home3/zd3/.zshrc",
            &format!("PS1='%# '; export T_RC=1\n{LOG}:rc\"\n"),
        );
        t.write("home3/zd3/.zlogin", &format!("{LOG}:login\"\n"));
        let (core, _rx) = test_zsh::spawn(&home3, None, &dir, true, true);
        let blocks = run(&core, CHECK);
        assert_eq!(
            last_output(&core, &blocks),
            Some(format!("[{}][1][1][unset]", zd3.display()))
        );
        let blocks = run(&core, ORDER_AND_CHILD);
        assert_eq!(
            last_output(&core, &blocks),
            Some(format!("env:profile:rc:login\n[{}][unset]", zd3.display()))
        );
        clean(&core, &zd3);
        drop(core);

        // Not a login shell (`zsh -i`): your .zshenv and .zshrc only.
        let (core, _rx) = test_zsh::spawn_as("-i", &home, None, &dir, true, true);
        let blocks = run(&core, ORDER_AND_CHILD);
        assert_eq!(
            last_output(&core, &blocks).as_deref(),
            Some("env:rc\n[unset][unset]")
        );
        clean(&core, &home);
    }

    #[test]
    fn your_files_see_your_zdotdir() {
        let t = TempDir::new("plugin-see");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        let see = "T_SEEN=\"${T_SEEN}[${ZDOTDIR-unset}]\"\n";
        // A plugin sees yours too.
        t.write("si/zsh/plugins.zsh", see);
        let zd = t.0.join("zd");
        for (folder, yours) in [("home", None), ("zd", Some(zd.as_path()))] {
            t.write(&format!("{folder}/.zprofile"), see);
            t.write(&format!("{folder}/.zshrc"), &format!("PS1='%# '\n{see}"));
            t.write(&format!("{folder}/.zlogin"), see);
            let home = t.0.join("home");
            std::fs::create_dir_all(&home).unwrap();
            let (core, _rx) = test_zsh::spawn(&home, yours, &dir, true, true);
            let blocks = run(&core, "print -r -- $T_SEEN");
            let one = yours.map_or("unset".to_string(), |zd| zd.display().to_string());
            // .zprofile, .zshrc, the plugin, .zlogin.
            assert_eq!(
                last_output(&core, &blocks),
                Some(format!("[{one}]").repeat(4))
            );
            drop(core);
        }

        // Your .zshenv sets ZDOTDIR empty: set, not unset, so zsh looks in `/` (as without
        // fuxx) and reads none of the files in HOME.
        t.write("home4/.zshenv", &format!("ZDOTDIR=\n{LOG}env\"\n"));
        for name in [".zprofile", ".zshrc", ".zlogin"] {
            t.write(&format!("home4/{name}"), &format!("{LOG}:wrong-{name}\"\n"));
        }
        let (core, _rx) = test_zsh::spawn(&t.0.join("home4"), None, &dir, true, true);
        let blocks = run(&core, "print -r -- \"[${+ZDOTDIR}][$ZDOTDIR][$T_LOG]\"");
        assert_eq!(last_output(&core, &blocks).as_deref(), Some("[1][][env]"));
    }

    #[test]
    fn your_nounset_doesnt_stop_plugin_mode() {
        // `setopt nounset` in your .zshenv: fuxx's files read nothing unset without a default.
        let t = TempDir::new("plugin-nounset");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        t.write("si/zsh/plugins.zsh", "T_LOG=\"${T_LOG}:plugin\"\n");
        t.write("home/.zshenv", "setopt nounset\nT_LOG=env\n");
        t.write("home/.zprofile", "T_LOG=\"${T_LOG}:profile\"\n");
        t.write("home/.zshrc", "PS1='%# '\nT_LOG=\"${T_LOG}:rc\"\n");
        t.write("home/.zlogin", "T_LOG=\"${T_LOG}:login\"\n");
        let (core, _rx) = test_zsh::spawn(&t.0.join("home"), None, &dir, false, true);
        core.input(b"print -r -- \"got-$T_LOG-${ZDOTDIR-unset}-$((6*7))\"\r");
        assert!(
            until(|| screen_text(&core).contains("got-env:profile:rc:plugin:login-unset-42")),
            "{}",
            screen_text(&core)
        );
    }

    #[test]
    fn plugins_load_after_your_zshrc_at_top_level() {
        let t = TempDir::new("plugin-top");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        let plugin = t.write(
            "plugin/a.zsh",
            &format!("typeset X=1; print -r -- \"plugin:$T_RC\"\n{LOG}:plugin\"\n"),
        );
        t.write(
            "si/zsh/plugins.zsh",
            &format!(
                "builtin source -- {}\n",
                shell_quote(&plugin.display().to_string())
            ),
        );
        startup_files(&t, "home");
        let (core, _rx) = test_zsh::spawn(&t.0.join("home"), None, &dir, true, true);
        assert!(
            until(|| screen_text(&core).contains("plugin:1")),
            "{}",
            screen_text(&core)
        );
        let blocks = run(&core, "print -r -- \"[$X][${(t)X}][$T_LOG]\"");
        assert_eq!(
            last_output(&core, &blocks).as_deref(),
            Some("[1][scalar][env:profile:rc:plugin:login]")
        );
    }

    #[test]
    fn a_broken_plugin_doesnt_stop_the_prompt() {
        let t = TempDir::new("plugin-broken");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        let bad = t.write("plugin/bad.zsh", "echo one\nbroken() {\n");
        let good = t.write("plugin/good.zsh", "Y=2\n");
        t.write(
            "si/zsh/plugins.zsh",
            &format!(
                "builtin source -- {}\nbuiltin source -- {}\n",
                shell_quote(&bad.display().to_string()),
                shell_quote(&good.display().to_string())
            ),
        );
        t.write("home/.zshrc", "PS1='%# '\n");
        let (core, _rx) = test_zsh::spawn(&t.0.join("home"), None, &dir, true, true);
        run(&core, "false");
        let blocks = run(&core, "print -r -- \"[$Y]\"");
        assert_eq!(outcomes(&blocks), [Outcome::Failed(1), Outcome::Ok]);
        assert_eq!(last_output(&core, &blocks).as_deref(), Some("[2]"));
    }

    #[test]
    fn a_broken_zshrc_still_loads_plugins() {
        let t = TempDir::new("plugin-broken-rc");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        let good = t.write("plugin/good.zsh", "Y=2\n");
        t.write(
            "si/zsh/plugins.zsh",
            &format!(
                "builtin source -- {}\n",
                shell_quote(&good.display().to_string())
            ),
        );
        startup_files(&t, "home");
        // An error that stops the rest of your .zshrc.
        t.write(
            "home/.zshrc",
            &format!("PS1='%# '\n{LOG}:rc\"\n: ${{T_NOPE?broken}}\n{LOG}:never\"\n"),
        );
        let (core, _rx) = test_zsh::spawn(&t.0.join("home"), None, &dir, true, true);
        let blocks = run(&core, "print -r -- \"[$Y][$T_LOG][${ZDOTDIR-unset}]\"");
        assert_eq!(
            last_output(&core, &blocks).as_deref(),
            Some("[2][env:profile:rc:login][unset]")
        );
    }

    #[test]
    fn plugins_without_command_blocks_send_no_marks() {
        let t = TempDir::new("plugin-no-marks");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        let plugin = t.write("plugin/z.zsh", "Z=3\n");
        t.write(
            "si/zsh/plugins.zsh",
            &format!(
                "builtin source -- {}\n",
                shell_quote(&plugin.display().to_string())
            ),
        );
        t.write("home/.zshrc", "PS1='%# '\n");
        let (core, _rx) = test_zsh::spawn(&t.0.join("home"), None, &dir, false, true);
        // The plugin's variable, fuxx's functions (none: no marks), and a sum, so the typed
        // line itself can't match.
        core.input(b"print -r -- \"got-$Z-[${(k)functions[(I)_fuxx*]}]-$((6*7))\"\r");
        assert!(
            until(|| screen_text(&core).contains("got-3-[]-42")),
            "{}",
            screen_text(&core)
        );
        std::thread::sleep(Duration::from_millis(300));
        assert!(core.blocks().is_empty(), "{:?}", core.blocks());
        assert!(!core.prompt_ready());
    }

    #[test]
    fn no_plugins_is_m11l_exactly() {
        let t = TempDir::new("plugin-none");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        // Markers in fuxx's own .zprofile, .zshrc and plugins.zsh: set only if zsh read them.
        let mark = |name: &str, what: &str| {
            let path = dir.join("zsh").join(name);
            let mut text = std::fs::read_to_string(&path).unwrap_or_default();
            text.push_str(&format!("\nT_FUXX=\"${{T_FUXX}}:{what}\"\n"));
            std::fs::write(&path, text).unwrap();
        };
        mark(".zprofile", "profile");
        mark(".zshrc", "rc");
        mark("plugins.zsh", "plugins");
        startup_files(&t, "home");
        let home = t.0.join("home");
        const FUXX: &str = "print -r -- \"[${T_FUXX-unset}]\"";

        let (core, _rx) = test_zsh::spawn(&home, None, &dir, true, false);
        let blocks = run(&core, CHECK);
        assert_eq!(
            last_output(&core, &blocks).as_deref(),
            Some("[][1][1][unset]")
        );
        let blocks = run(&core, ORDER_AND_CHILD);
        assert_eq!(
            last_output(&core, &blocks).as_deref(),
            Some("env:profile:rc:login\n[unset][unset]")
        );
        let blocks = run(&core, FUXX);
        assert_eq!(last_output(&core, &blocks).as_deref(), Some("[unset]"));
        drop(core);

        // With plugins on, the same markers show (the check above can fail).
        let (core, _rx) = test_zsh::spawn(&home, None, &dir, true, true);
        let blocks = run(&core, FUXX);
        assert_eq!(
            last_output(&core, &blocks).as_deref(),
            Some("[:profile:plugins:rc]")
        );
    }

    #[test]
    fn an_older_fuxxs_environment_still_gets_marks() {
        // An older fuxx (M11l) sharing this folder sets only ZDOTDIR: neither flag. env_for
        // never injects without one, so that means command blocks, as it did then.
        let t = TempDir::new("plugin-older");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        t.write("si/zsh/plugins.zsh", "T_PM=1\n");
        startup_files(&t, "home");
        let zdotdir = ("ZDOTDIR".to_string(), dir.join("zsh").display().to_string());
        let (core, _rx) = test_zsh::spawn_with("-il", &t.0.join("home"), None, &[zdotdir]);
        let blocks = run(&core, ORDER_AND_CHILD);
        assert_eq!(
            last_output(&core, &blocks).as_deref(),
            Some("env:profile:rc:login\n[unset][unset]")
        );
        let blocks = run(&core, "print -r -- \"[${T_PM-unset}]\"");
        assert_eq!(last_output(&core, &blocks).as_deref(), Some("[unset]"));
    }

    #[test]
    fn a_non_interactive_zsh_ignores_plugin_mode() {
        let t = TempDir::new("plugin-non-interactive");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        t.write("si/zsh/plugins.zsh", "print -r -- plugins-loaded\n");
        startup_files(&t, "home");
        startup_files(&t, "zd");
        let home = t.0.join("home");
        let zd = t.0.join("zd");
        let script = "print -r -- \"[${ZDOTDIR-unset}][$T_LOG][${FUXX_ZSH_MARKS-unset}]\
                      [${FUXX_ZSH_PLUGINS-unset}][${(k)parameters[(I)_fuxx_*]}]\"";
        for (yours, flag, log) in [
            (None, "-c", "env"),
            (None, "-lc", "env:profile:login"),
            (Some(&zd), "-c", "env"),
            (Some(&zd), "-lc", "env:profile:login"),
        ] {
            let mut cmd = std::process::Command::new("/bin/zsh");
            cmd.arg(flag).arg(script).env_clear();
            cmd.env("HOME", &home)
                .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
            if let Some(zd) = yours {
                cmd.env("ZDOTDIR", zd);
            }
            let zdo = yours.map(|zd| zd.as_os_str());
            for (key, value) in env_for(Path::new("/bin/zsh"), zdo, true, true, Some(&dir)) {
                cmd.env(key, value);
            }
            let out = cmd.output().unwrap();
            let one = yours.map_or("unset".to_string(), |zd| zd.display().to_string());
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                format!("[{one}][{log}][unset][unset][]\n"),
                "{flag} {yours:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    #[test]
    fn a_theme_that_rebuilds_ps1_still_gets_marks() {
        let t = TempDir::new("theme");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        // As powerlevel10k and pure do: a precmd hook registered after the integration's,
        // building PS1 again before every prompt.
        t.write(
            "home/.zshrc",
            "theme() { PS1=\"%# $RANDOM \" }\nprecmd_functions+=(theme)\n",
        );
        let (core, _rx) = spawn_zsh(&t.0.join("home"), None, &dir);
        run(&core, "echo one");
        run(&core, "false");
        let blocks = run(&core, "echo three");
        assert_eq!(cmds(&blocks), ["echo one", "false", "echo three"]);
        assert_eq!(
            outcomes(&blocks),
            [Outcome::Ok, Outcome::Failed(1), Outcome::Ok]
        );
        assert!(core.prompt_ready());
    }

    #[test]
    fn a_two_line_prompt_and_a_nested_zsh() {
        let t = TempDir::new("nested");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        t.write("home/.zshrc", "PROMPT=$'top\\n%# '\n");
        let (core, _rx) = spawn_zsh(&t.0.join("home"), None, &dir);
        run(&core, "echo hi");
        // A nested zsh reads your files only: no marks inside, and the outer shell's go on
        // once it exits.
        assert!(until(|| core.prompt_ready()));
        core.input(b"zsh\r");
        let tops = |core: &TerminalCore| screen_text(core).matches("top").count();
        assert!(until(|| tops(&core) == 3), "{}", screen_text(&core));
        core.input(b"exit\r");
        assert!(until(|| core.blocks().len() == 2 && core.prompt_ready()));
        let blocks = run(&core, "true");
        assert_eq!(cmds(&blocks), ["echo hi", "zsh", "true"]);
        assert_eq!(outcomes(&blocks), [Outcome::Ok; 3]);
        // The prompt is two lines: one above the command row.
        for b in &blocks {
            assert_eq!(b.cmd_row.0 - b.prompt_top.0, 1, "{b:?}");
        }
    }

    #[test]
    fn your_zle_line_init_widget_still_runs_at_every_prompt() {
        let t = TempDir::new("line-init");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        // Your own zle-line-init (a plain widget, as many configs have), and a theme that
        // builds PS1 again after fuxx's hook: fuxx's line-init hook joins yours, never
        // replaces it.
        t.write(
            "home/.zshrc",
            "typeset -gi T_LI=0\nzle-line-init() { T_LI+=1 }\nzle -N zle-line-init\n\
             theme() { PS1=\"%# $RANDOM \"; precmd_functions=(${precmd_functions:#theme} theme) }\n\
             precmd_functions+=(theme)\n",
        );
        let (core, _rx) = spawn_zsh(&t.0.join("home"), None, &dir);
        run(&core, "print -r -- $T_LI");
        let blocks = run(&core, "print -r -- $T_LI");
        assert_eq!(cmds(&blocks).len(), 2);
        let counts: Vec<String> = blocks
            .iter()
            .map(|b| core.block_output(&b.id).unwrap_or_default())
            .collect();
        assert_eq!(counts, ["1", "2"]);
    }

    #[test]
    fn iterm2_marks_as_well_change_nothing() {
        let t = TempDir::new("iterm");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        // iTerm2's integration's marks, sent as well: D in its precmd (every prompt), its own
        // A and B around PS1, and a C without the command.
        t.write(
            "home/.zshrc",
            "it_precmd() { local s=$?; printf '\\e]133;D;%s\\a' $s\n\
               PS1=$'%{\\e]133;A\\a%}%# %{\\e]133;B\\a%}' }\n\
             it_preexec() { printf '\\e]133;C;\\a' }\n\
             precmd_functions+=(it_precmd); preexec_functions+=(it_preexec)\n",
        );
        let (core, _rx) = spawn_zsh(&t.0.join("home"), None, &dir);
        run(&core, "echo hi");
        let blocks = run(&core, "false");
        assert_eq!(cmds(&blocks), ["echo hi", "false"]);
        assert_eq!(outcomes(&blocks), [Outcome::Ok, Outcome::Failed(1)]);
        assert_eq!(core.block_output(&blocks[0].id).as_deref(), Some("hi"));
    }

    #[test]
    fn what_other_precmd_hooks_print_is_not_the_commands_output() {
        let t = TempDir::new("took");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        // Timing plugins' hooks: one put first in precmd_functions, one added at the end.
        t.write(
            "home/.zshrc",
            "PS1='%# '\nfront() { print \"took 3s\" }; back() { print \"took 4s\" }\n\
             precmd_functions=(front $precmd_functions back)\n",
        );
        let (core, _rx) = spawn_zsh(&t.0.join("home"), None, &dir);
        run(&core, "echo hi");
        let blocks = run(&core, "echo two");
        assert_eq!(cmds(&blocks), ["echo hi", "echo two"]);
        for (b, out) in blocks.iter().zip(["hi", "two"]) {
            assert_eq!(core.block_output(&b.id).as_deref(), Some(out));
        }
    }

    #[test]
    fn a_precmd_function_still_prints_into_the_block() {
        // A limit: zsh runs a function named `precmd` before every precmd_functions hook, so
        // what it prints comes before D, and is the command's output.
        let t = TempDir::new("precmd-fn");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        t.write("home/.zshrc", "PS1='%# '\nprecmd() { print \"took 3s\" }\n");
        let (core, _rx) = spawn_zsh(&t.0.join("home"), None, &dir);
        let blocks = run(&core, "echo hi");
        assert_eq!(
            core.block_output(&blocks[0].id).as_deref(),
            Some("hi\ntook 3s")
        );
    }

    #[test]
    fn exec_zsh_leaves_no_block_running() {
        // `exec zsh` (oh-my-zsh's `omz reload`): the new zsh reads your files only, without
        // fuxx's hooks. No block is left running, and no later command makes one (the
        // integration is lost until a new terminal: a limit). The same with an assignment
        // before it.
        for (i, line) in ["exec zsh", "FOO=1 exec zsh"].into_iter().enumerate() {
            let t = TempDir::new(&format!("exec-zsh-{i}"));
            let dir = t.0.join("si");
            install(&dir).unwrap();
            t.write("home/.zshrc", "PS1='%# '\n");
            let (core, _rx) = spawn_zsh(&t.0.join("home"), None, &dir);
            let blocks = run(&core, "echo a");
            assert_eq!(outcomes(&blocks), [Outcome::Ok]);
            assert!(until(|| core.prompt_ready() && core.cursor_at_input_start()));
            core.input(format!("{line}\r").as_bytes());
            core.input(b"echo after-$((6*7))\r");
            assert!(
                until(|| screen_text(&core).contains("after-42")),
                "{line}: {}",
                screen_text(&core)
            );
            std::thread::sleep(Duration::from_millis(300));
            let blocks = core.blocks();
            assert_eq!(cmds(&blocks), ["echo a"], "{line}: {blocks:?}");
            assert_eq!(outcomes(&blocks), [Outcome::Ok], "{line}");
        }
    }

    #[test]
    fn no_add_zle_hook_widget_is_no_error() {
        // zsh before 5.3 has no add-zle-hook-widget (here: an empty fpath, so it can't be
        // loaded): the line-init hook is left out, quietly, and blocks still come.
        let t = TempDir::new("no-azhw");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        t.write("home/.zshrc", "PS1='%# '; fpath=()\n");
        let (core, _rx) = spawn_zsh(&t.0.join("home"), None, &dir);
        run(&core, "echo hi");
        let blocks = run(&core, "false");
        assert_eq!(outcomes(&blocks), [Outcome::Ok, Outcome::Failed(1)]);
        let screen = screen_text(&core);
        assert!(!screen.contains("not found"), "{screen}");
        assert!(!screen.contains("add-zle-hook-widget"), "{screen}");
    }

    #[test]
    fn a_pure_like_theme_gets_one_prompt_start() {
        let t = TempDir::new("pure");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        // As pure does: before every prompt, a precmd hook (registered after ours) keeps what
        // follows PROMPT's last newline and puts its own first line above it.
        t.write(
            "home/.zshrc",
            "PROMPT='%# '\npurish() { PROMPT=\"pre\"$'\\n'\"${PROMPT##*$'\\n'}\" }\n\
             precmd_functions+=(purish)\n",
        );
        let (core, _rx) = spawn_zsh(&t.0.join("home"), None, &dir);
        run(&core, "echo one");
        run(&core, "echo two");
        let blocks = run(&core, "echo three");
        assert_eq!(cmds(&blocks), ["echo one", "echo two", "echo three"]);
        for b in &blocks {
            // The prompt is two lines, "pre" and the command row.
            assert_eq!(b.cmd_row.0 - b.prompt_top.0, 1, "{b:?}");
        }
        // The next prompt's first line is not the output of the command before it.
        for (b, out) in blocks.iter().zip(["one", "two", "three"]) {
            assert_eq!(core.block_output(&b.id).as_deref(), Some(out));
        }
    }

    // --- powerlevel10k ---

    /// A stand-in for powerlevel10k's hooks, as its `internal/p10k.zsh` has them: at its first
    /// prompt it reads `POWERLEVEL9K_TERM_SHELL_INTEGRATION` (`_p9k_init_params`) and, when
    /// true, puts A and B in every prompt it builds and sends its own C and D
    /// (`_p9k_iterm2_precmd`/`_preexec`, a C and D for an empty line too); before every prompt
    /// it builds `PROMPT` from scratch (`_p9k_set_prompt`) and moves its hook after the others
    /// (`_p9k_precmd`). `SIM_TRANSIENT` = `POWERLEVEL9K_TRANSIENT_PROMPT=always`: the prompt
    /// is drawn again, shorter, when Return is pressed.
    const SIM_P10K: &str = r#"# A faithful stand-in for powerlevel10k's hooks (internal/p10k.zsh: _p9k_setup, _p9k_precmd_impl's
# reordering, _p9k_set_prompt, _p9k_iterm2_precmd/_preexec, _p9k_on_widget_zle-line-finish).
setopt prompt_subst
p10k() { :; }
_sim_precmd() {
  local st=$?
  if (( ! ${+_POWERLEVEL9K_TERM_SHELL_INTEGRATION} )); then   # _p9k_init_params, first prompt
    typeset -gi _POWERLEVEL9K_TERM_SHELL_INTEGRATION=0
    [[ $POWERLEVEL9K_TERM_SHELL_INTEGRATION == true ]] && _POWERLEVEL9K_TERM_SHELL_INTEGRATION=1
    (( _POWERLEVEL9K_TERM_SHELL_INTEGRATION )) && typeset -gi _p9k__iterm_cmd=0   # _p9k_init
  fi
  if (( ${+_p9k__iterm_cmd} )); then                          # _p9k_iterm2_precmd
    (( _p9k__iterm_cmd == 1 )) && print -n '\e]133;C;\a'
    (( _p9k__iterm_cmd )) && printf '\e]133;D;%s\a' $st
    _p9k__iterm_cmd=1
  fi
  local a= b=
  (( _POWERLEVEL9K_TERM_SHELL_INTEGRATION )) && a=$'%{\e]133;A\a%}' b=$'%{\e]133;B\a%}'
  typeset -g _sim_a=$a _sim_b=$b
  PROMPT='${:-}%b'$a'%~ ${:-sim}%# '$b                        # _p9k_set_prompt: from scratch
  [[ $precmd_functions[-1] == _sim_precmd ]] ||               # every prompt
    precmd_functions=(${precmd_functions:#_sim_precmd} _sim_precmd)
}
_sim_preexec() { (( ! ${+_p9k__iterm_cmd} )) || { print -n '\e]133;C;\a'; _p9k__iterm_cmd=2 } }
precmd_functions+=(_sim_precmd)
preexec_functions+=(_sim_preexec)
if [[ -n $SIM_TRANSIENT ]]; then                              # POWERLEVEL9K_TRANSIENT_PROMPT=always
  _sim_finish() { PROMPT=$_sim_a'%# '$_sim_b; zle .reset-prompt }
  autoload -Uz add-zle-hook-widget; add-zle-hook-widget line-finish _sim_finish
fi
"#;

    /// The prompt's row, counted from the top of the scrollback (stable while it scrolls).
    fn prompt_row(core: &TerminalCore) -> Option<i64> {
        core.live_prompt_from_top()
    }

    /// Send `keys` at the ready prompt that should give a new prompt and no block (an empty
    /// Return, Ctrl-C): wait for the new prompt to be ready, then check no block came.
    fn no_block_for(core: &TerminalCore, keys: &[u8]) {
        assert!(
            until(|| core.prompt_ready() && core.cursor_at_input_start()),
            "no ready prompt before {keys:?}; screen:\n{}",
            screen_text(core)
        );
        let before = (core.blocks(), prompt_row(core));
        core.input(keys);
        let moved = until(|| {
            core.prompt_ready() && core.cursor_at_input_start() && prompt_row(core) != before.1
        });
        assert!(
            moved,
            "no new prompt after {keys:?}; screen:\n{}",
            screen_text(core)
        );
        // Anything late (a D) has had time to come.
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(core.blocks(), before.0, "{keys:?} made a block");
    }

    /// `echo hi`, `false`, `sleep 1` (seen running, then done), an empty Return, Ctrl-C over
    /// typed text, and `true`: four blocks, none for the Return or Ctrl-C.
    fn p10k_scenario(core: &TerminalCore) {
        run(core, "echo hi");
        run(core, "false");
        assert!(until(|| core.prompt_ready() && core.cursor_at_input_start()));
        core.input(b"sleep 1\r");
        let running = until(|| {
            let blocks = core.blocks();
            blocks.len() == 3 && blocks[2].outcome == Outcome::Running
        });
        assert!(
            running,
            "{:?}; screen:\n{}",
            core.blocks(),
            screen_text(core)
        );
        assert!(until(|| {
            core.blocks()
                .get(2)
                .is_some_and(|b| b.outcome == Outcome::Ok)
                && core.prompt_ready()
        }));
        no_block_for(core, b"\r");
        no_block_for(core, b"abc\x03");
        let blocks = run(core, "true");
        assert_eq!(cmds(&blocks), ["echo hi", "false", "sleep 1", "true"]);
        assert_eq!(
            outcomes(&blocks),
            [Outcome::Ok, Outcome::Failed(1), Outcome::Ok, Outcome::Ok]
        );
        assert_eq!(core.block_output(&blocks[0].id).as_deref(), Some("hi"));
        for b in &blocks {
            assert_eq!(b.prompt_top, b.cmd_row, "a one-line prompt: {b:?}");
        }
    }

    /// zsh with the powerlevel10k stand-in, `before` and `after` in `.zshrc` around it.
    fn sim_p10k(name: &str, before: &str, after: &str) -> (TempDir, TerminalCore) {
        let t = TempDir::new(name);
        let dir = t.0.join("si");
        install(&dir).unwrap();
        t.write("home/sim-p10k.zsh", SIM_P10K);
        t.write(
            "home/.zshrc",
            &format!("{before}\nsource ~/sim-p10k.zsh\n{after}\n"),
        );
        let (core, _rx) = spawn_zsh(&t.0.join("home"), None, &dir);
        (t, core)
    }

    #[test]
    fn p10k_gets_command_blocks() {
        let (_t, core) = sim_p10k("p10k", "", "");
        p10k_scenario(&core);
        let blocks = run(&core, FIRST_HOOK);
        assert_eq!(
            core.block_output(&blocks[4].id).as_deref(),
            Some("_fuxx_done")
        );
    }

    #[test]
    fn p10k_with_its_marks_turned_off_gets_command_blocks() {
        // Your own setting is kept: fuxx marks the prompt itself, after p10k's hook.
        let (_t, core) = sim_p10k("p10k-off", "", "POWERLEVEL9K_TERM_SHELL_INTEGRATION=false");
        p10k_scenario(&core);
        let blocks = run(&core, "print -r -- $POWERLEVEL9K_TERM_SHELL_INTEGRATION");
        assert_eq!(core.block_output(&blocks[4].id).as_deref(), Some("false"));
    }

    #[test]
    fn p10k_with_a_transient_prompt_gets_command_blocks() {
        let (_t, core) = sim_p10k("p10k-transient", "SIM_TRANSIENT=1", "");
        p10k_scenario(&core);
    }

    #[test]
    fn p10k_transient_prompt_without_its_marks_loses_the_blocks() {
        // A limit: with p10k's marks turned off by you, its transient prompt is drawn again on
        // Return, after every hook fuxx has, without A and B; the redraw drops the prompt's tag
        // before the command starts, so no command is a block. The shell works as ever.
        let (_t, core) = sim_p10k(
            "p10k-off-transient",
            "SIM_TRANSIENT=1",
            "POWERLEVEL9K_TERM_SHELL_INTEGRATION=false",
        );
        for line in ["echo hi", "false", "true"] {
            assert!(until(|| core.prompt_ready() && core.cursor_at_input_start()));
            let row = prompt_row(&core);
            core.input(format!("{line}\r").as_bytes());
            assert!(until(|| core.prompt_ready() && prompt_row(&core) != row));
        }
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(core.blocks(), Vec::<Block>::new());
        assert!(screen_text(&core).contains("hi"), "{}", screen_text(&core));
    }

    /// Against a real powerlevel10k (its folder in `FUXX_TEST_P10K`), with its lean two-line
    /// config, gitstatus off, the folder shortened to its last part (the config's default
    /// looks through every parent of the long temp path, seconds per prompt), and its cache in
    /// the temp folder:
    /// `FUXX_TEST_P10K=/path/to/powerlevel10k cargo test real_p10k -- --ignored`.
    #[test]
    #[ignore = "needs a powerlevel10k install in FUXX_TEST_P10K"]
    fn real_p10k_gets_command_blocks() {
        let p10k = std::env::var("FUXX_TEST_P10K").expect("FUXX_TEST_P10K");
        let t = TempDir::new("real-p10k");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        let home = t.0.join("home");
        let cache = shell_quote(&t.0.join("cache").display().to_string());
        t.write("home/.zshenv", &format!("export XDG_CACHE_HOME={cache}\n"));
        let p = shell_quote(&p10k);
        t.write(
            "home/.zshrc",
            &format!(
                "if [[ -r \"$XDG_CACHE_HOME/p10k-instant-prompt-${{(%):-%n}}.zsh\" ]]; then\n\
                   source \"$XDG_CACHE_HOME/p10k-instant-prompt-${{(%):-%n}}.zsh\"\n\
                 fi\n\
                 source {p}/powerlevel10k.zsh-theme\n\
                 source {p}/config/p10k-lean.zsh\n\
                 POWERLEVEL9K_DISABLE_GITSTATUS=true\n\
                 POWERLEVEL9K_SHORTEN_STRATEGY=truncate_to_last\n\
                 POWERLEVEL9K_DISABLE_CONFIGURATION_WIZARD=true\n"
            ),
        );
        // Twice: the first start writes the instant prompt, the second starts with it.
        for _ in 0..2 {
            let (core, _rx) = spawn_zsh(&home, None, &dir);
            run(&core, "echo hi");
            run(&core, "false");
            no_block_for(&core, b"\r");
            no_block_for(&core, b"abc\x03");
            let blocks = run(&core, "true");
            assert_eq!(cmds(&blocks), ["echo hi", "false", "true"]);
            assert_eq!(
                outcomes(&blocks),
                [Outcome::Ok, Outcome::Failed(1), Outcome::Ok]
            );
            assert_eq!(core.block_output(&blocks[0].id).as_deref(), Some("hi"));
            for b in &blocks {
                // The lean config's prompt is two lines.
                assert_eq!(b.cmd_row.0 - b.prompt_top.0, 1, "{b:?}");
            }
            // p10k puts its own hooks first and last at every prompt: D is still sent first.
            let blocks = run(&core, FIRST_HOOK);
            assert_eq!(
                core.block_output(&blocks[3].id).as_deref(),
                Some("_fuxx_done")
            );
        }
    }

    /// Prints the first precmd hook.
    const FIRST_HOOK: &str = "print -r -- ${precmd_functions[1]}";

    #[test]
    fn marks_go_to_the_terminal_not_a_redirected_stdout() {
        let t = TempDir::new("tty");
        let dir = t.0.join("si");
        install(&dir).unwrap();
        t.write("home/.zshrc", "PS1='%# '\n");
        let home = t.0.join("home");
        let (core, _rx) = spawn_zsh(&home, None, &dir);
        run(&core, "exec >out.txt");
        let blocks = run(&core, "echo hi");
        assert_eq!(cmds(&blocks), ["exec >out.txt", "echo hi"]);
        assert_eq!(outcomes(&blocks), [Outcome::Ok; 2]);
        // The command's output went to the file, and nothing of fuxx's with it.
        assert_eq!(
            std::fs::read_to_string(home.join("out.txt")).unwrap(),
            "hi\n"
        );
        // stdout closed: the marks still reach the terminal, and nothing complains.
        run(&core, "exec >&-");
        let blocks = run(&core, "true");
        assert_eq!(cmds(&blocks)[2..], ["exec >&-", "true"]);
        assert!(
            !screen_text(&core).contains("write error"),
            "{}",
            screen_text(&core)
        );
    }

    #[test]
    fn a_stray_saved_zdotdir_is_dropped() {
        // fuxx's own environment has FUXX_ZSH_ZDOTDIR but no ZDOTDIR (left by something
        // else): the child must not get it, or fuxx's .zshenv would "restore" it.
        let mut env = crate::pty::ShellEnv {
            base: None,
            set: Vec::new(),
            remove: Vec::new(),
        };
        let i = Some(Path::new("/i"));
        add_login_shell_env(&mut env, OsStr::new("/bin/zsh"), None, true, false, i);
        assert_eq!(
            env.set,
            kv(&[("ZDOTDIR", "/i/zsh"), ("FUXX_ZSH_MARKS", "1")])
        );
        assert_eq!(env.remove, ["FUXX_ZSH_ZDOTDIR"]);
        // With yours: set, not removed.
        let mut env = crate::pty::ShellEnv {
            base: None,
            set: Vec::new(),
            remove: Vec::new(),
        };
        add_login_shell_env(
            &mut env,
            OsStr::new("/bin/zsh"),
            Some(OsStr::new("/u")),
            true,
            false,
            i,
        );
        assert_eq!(
            env.set,
            kv(&[
                ("ZDOTDIR", "/i/zsh"),
                ("FUXX_ZSH_ZDOTDIR", "/u"),
                ("FUXX_ZSH_MARKS", "1")
            ])
        );
        assert!(env.remove.is_empty());
        // Not injected (bash, or off): nothing changes.
        let mut env = crate::pty::ShellEnv {
            base: None,
            set: Vec::new(),
            remove: Vec::new(),
        };
        add_login_shell_env(&mut env, OsStr::new("/bin/bash"), None, true, true, i);
        add_login_shell_env(&mut env, OsStr::new("/bin/zsh"), None, true, true, None);
        add_login_shell_env(&mut env, OsStr::new("/bin/zsh"), None, false, false, i);
        assert!(env.set.is_empty() && env.remove.is_empty());
    }
}
