//! A child process on a pseudo-terminal (portable-pty, as in v1).

use std::io::{Read, Write};
use std::path::Path;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::terminal::{CellPx, TermDims};

/// What a shell's environment gets on top of the one fuxx inherited: variables to drop, then
/// variables to set.
#[derive(Debug)]
pub struct ShellEnv {
    pub set: Vec<(String, String)>,
    pub remove: Vec<&'static str>,
}

/// The user's login shell (`$SHELL`, run as `-zsh` etc.), started in `cwd` or `$HOME`, with `env`
/// applied.
pub fn shell_command(cwd: Option<&Path>, env: &ShellEnv) -> CommandBuilder {
    let mut cmd = CommandBuilder::new_default_prog();
    match cwd {
        Some(dir) => cmd.cwd(dir),
        None => {
            if let Some(home) = std::env::var_os("HOME") {
                cmd.cwd(home);
            }
        }
    }
    // Always set these: an app launched from Finder has no TERM, one launched from another
    // terminal inherits *that* terminal's TERM. `alacritty` terminfo isn't on stock macOS.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    apply_env(&mut cmd, env);
    cmd
}

/// portable-pty starts from a copy of fuxx's own environment, so dropping a variable takes an
/// explicit remove.
pub(crate) fn apply_env(cmd: &mut CommandBuilder, env: &ShellEnv) {
    for key in &env.remove {
        cmd.env_remove(key);
    }
    for (key, value) in &env.set {
        cmd.env(key, value);
    }
}

pub fn pty_size(dims: TermDims, cell: CellPx) -> PtySize {
    PtySize {
        rows: dims.rows as u16,
        cols: dims.cols as u16,
        pixel_width: (dims.cols as u16).saturating_mul(cell.width),
        pixel_height: (dims.rows as u16).saturating_mul(cell.height),
    }
}

pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
}

impl Pty {
    /// Spawn `cmd`; returns the handle plus the output reader and the input writer, each for a
    /// dedicated thread. Keep the writer for the whole session: dropping portable-pty's writer
    /// sends EOF (^D) to the child.
    pub fn spawn(
        cmd: CommandBuilder,
        dims: TermDims,
    ) -> Result<(Pty, Box<dyn Read + Send>, Box<dyn Write + Send>), String> {
        let pair = native_pty_system()
            .openpty(pty_size(dims, CellPx::default()))
            .map_err(|e| e.to_string())?;
        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        // Drop our copy of the slave: the reader only reaches EOF once every slave handle closes.
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        Ok((
            Pty {
                master: pair.master,
                child,
            },
            reader,
            writer,
        ))
    }

    /// Tell the child its new size (it receives SIGWINCH).
    pub fn resize(&self, dims: TermDims, cell: CellPx) -> Result<(), String> {
        self.master
            .resize(pty_size(dims, cell))
            .map_err(|e| e.to_string())
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Closing a session must not leave its shell/agent running.
        let _ = self.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_env_removes_then_sets() {
        let mut cmd = CommandBuilder::new("sh");
        cmd.env_clear();
        cmd.env("GONE", "inherited");
        cmd.env("KEPT", "inherited");
        cmd.env("SET", "inherited");
        apply_env(
            &mut cmd,
            &ShellEnv {
                set: vec![("SET".into(), "ours".into())],
                remove: vec!["GONE", "SET"],
            },
        );
        assert_eq!(cmd.get_env("GONE"), None);
        assert_eq!(
            cmd.get_env("KEPT").and_then(|v| v.to_str()),
            Some("inherited")
        );
        assert_eq!(cmd.get_env("SET").and_then(|v| v.to_str()), Some("ours"));
        // The real command applies it too.
        let cmd = shell_command(
            None,
            &ShellEnv {
                set: Vec::new(),
                remove: vec!["FUXX_SESSION"],
            },
        );
        assert_eq!(cmd.get_env("FUXX_SESSION"), None);
        assert_eq!(
            cmd.get_env("TERM").and_then(|v| v.to_str()),
            Some("xterm-256color")
        );
    }

    #[test]
    fn pty_size_carries_pixels() {
        let s = pty_size(
            TermDims { cols: 80, rows: 24 },
            CellPx {
                width: 8,
                height: 17,
            },
        );
        assert_eq!(
            (s.cols, s.rows, s.pixel_width, s.pixel_height),
            (80, 24, 640, 408)
        );
    }
}
