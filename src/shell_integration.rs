//! fuxx's zsh integration (M11l): the two scripts that make zsh send the command marks
//! (`shell_marks`), compiled in and written at startup to
//! `~/Library/Application Support/fuxx/shell-integration/zsh/`, and the environment that has a
//! login zsh read them first (`ZDOTDIR`, Ghostty's and Kitty's method). Your dotfiles are never
//! touched: fuxx's `.zshenv` puts your `ZDOTDIR` back and sources your own `.zshenv`, and zsh then
//! reads the rest of your files from your own folder as always.

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

/// Each script, by its file name in `<dir>/zsh/`.
const FILES: [(&str, &str); 2] = [(".zshenv", ZSHENV), ("fuxx-integration.zsh", INTEGRATION)];

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

/// Where the scripts were installed at startup; none before, or if that failed.
pub fn installed() -> Option<&'static Path> {
    INSTALLED.get().map(PathBuf::as_path)
}

/// What a new shell's environment gets so zsh reads the integration: `ZDOTDIR=<dir>/zsh`, and
/// `FUXX_ZSH_ZDOTDIR` with yours when fuxx's own environment has one. Nothing unless `shell`'s
/// file name is exactly `zsh`, `enabled` (`command-blocks`) and the scripts are installed.
pub fn env_for(
    shell: &Path,
    user_zdotdir: Option<&OsStr>,
    enabled: bool,
    dir: Option<&Path>,
) -> Vec<(String, String)> {
    let Some(dir) = dir.filter(|_| enabled) else {
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
    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{Block, Outcome};
    use crate::shell_marks::percent_decode;
    use crate::terminal::{TermDims, TermEvent, TerminalCore};
    use crate::theme::Theme;
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};
    use portable_pty::CommandBuilder;
    use std::time::{Duration, Instant};

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
        assert_eq!(env_for(zsh, None, true, i), kv(&[("ZDOTDIR", "/i/zsh")]));
        assert_eq!(
            env_for(zsh, Some(OsStr::new("/u/zd")), true, i),
            kv(&[("ZDOTDIR", "/i/zsh"), ("FUXX_ZSH_ZDOTDIR", "/u/zd")])
        );
        assert_eq!(
            env_for(Path::new("/opt/homebrew/bin/zsh"), None, true, i),
            kv(&[("ZDOTDIR", "/i/zsh")])
        );
        for other in ["/bin/bash", "/usr/local/bin/fish", "/bin/sh", "/bin/zsh5"] {
            assert_eq!(env_for(Path::new(other), None, true, i), [], "{other}");
        }
        assert_eq!(env_for(zsh, Some(OsStr::new("/u/zd")), false, i), []);
        assert_eq!(env_for(zsh, Some(OsStr::new("/u/zd")), true, None), []);
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
        for name in [".zshenv", "fuxx-integration.zsh"] {
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
        assert_eq!(names, [".zshenv", "fuxx-integration.zsh"]);
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
        let out = std::process::Command::new("/bin/zsh")
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
        let marks = marks_in(&zsh_f(&t, "false; _fuxx_precmd"));
        assert_eq!(marks, Vec::<String>::new());
        // A command ran: D with its status, once.
        let marks = marks_in(&zsh_f(
            &t,
            "_fuxx_preexec 'false'; (exit 3); _fuxx_precmd; true; _fuxx_precmd",
        ));
        assert_eq!(marks, ["C;cmdline_url=false", "D;3"]);
    }

    #[test]
    fn precmd_marks_the_prompt_once_and_moves_last_once() {
        let t = TempDir::new("prompt");
        let out = zsh_f(
            &t,
            "PS1='%# '; theme() { :; }; precmd_functions+=(theme)\n\
             _fuxx_precmd; print -r -- \"[${(V)PS1}]\"\n\
             _fuxx_precmd; print -r -- \"[${(V)PS1}]\"\n\
             print -r -- \"[$precmd_functions]\"\n\
             PS1='end%'; _fuxx_precmd; print -r -- \"[${(V)PS1}]\"\n\
             source \"$self\"; print -r -- \"[$precmd_functions][$preexec_functions]\"",
        );
        let text = String::from_utf8_lossy(&out);
        let lines: Vec<&str> = text.lines().collect();
        let marked = "[%{^[]133;A^G%}%# %{^[]133;B^G%}]";
        assert_eq!(lines[0], marked);
        // Already marked: unchanged.
        assert_eq!(lines[1], marked);
        assert_eq!(lines[2], "[theme _fuxx_precmd]");
        // A lone % at the end is doubled, so it can't eat the B mark's `%{`.
        assert_eq!(lines[3], "[%{^[]133;A^G%}end%%%{^[]133;B^G%}]");
        // Sourced again: nothing added twice.
        assert_eq!(lines[4], "[theme _fuxx_precmd][_fuxx_preexec]");
    }

    // --- A real zsh in a terminal ---

    /// Poll `f` until it holds or 10 s pass.
    fn until(mut f: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// `/bin/zsh -il` in a terminal, `HOME` = `home`, with only the environment a fuxx terminal
    /// would add (`env_for`) on a small fixed one, and `ZDOTDIR` = `user_zdotdir` if given.
    fn spawn_zsh(
        home: &Path,
        user_zdotdir: Option<&Path>,
        dir: &Path,
    ) -> (TerminalCore, UnboundedReceiver<TermEvent>) {
        let mut cmd = CommandBuilder::new("/bin/zsh");
        cmd.arg("-il");
        cmd.env_clear();
        cmd.env("HOME", home);
        cmd.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        cmd.env("TERM", "xterm-256color");
        cmd.env("LANG", "en_US.UTF-8");
        if let Some(zd) = user_zdotdir {
            cmd.env("ZDOTDIR", zd);
        }
        let zd = user_zdotdir.map(Path::as_os_str);
        for (key, value) in env_for(Path::new("/bin/zsh"), zd, true, Some(dir)) {
            cmd.env(key, value);
        }
        cmd.cwd(home);
        let (tx, rx) = unbounded();
        let core = TerminalCore::spawn(cmd, TermDims { cols: 80, rows: 30 }, tx, Theme::default())
            .expect("spawn zsh");
        (core, rx)
    }

    fn screen_text(core: &TerminalCore) -> String {
        core.snapshot()
            .lines
            .iter()
            .map(|l| l.iter().map(|c| c.ch).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Type `line` and Return at the ready prompt, then wait for its block to finish and the
    /// next prompt to be ready. The blocks then.
    fn run(core: &TerminalCore, line: &str) -> Vec<Block> {
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
}
