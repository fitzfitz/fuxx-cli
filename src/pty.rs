//! A child process on a pseudo-terminal (portable-pty, as in v1).

use std::io::{Read, Write};
use std::path::Path;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::terminal::TermDims;

/// The user's login shell (`$SHELL`, run as `-zsh` etc.), started in `cwd` or `$HOME`.
pub fn shell_command(cwd: Option<&Path>) -> CommandBuilder {
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
    cmd
}

fn pty_size(dims: TermDims) -> PtySize {
    PtySize {
        rows: dims.rows as u16,
        cols: dims.cols as u16,
        pixel_width: 0,
        pixel_height: 0,
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
            .openpty(pty_size(dims))
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
    pub fn resize(&self, dims: TermDims) -> Result<(), String> {
        self.master
            .resize(pty_size(dims))
            .map_err(|e| e.to_string())
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Closing a session must not leave its shell/agent running.
        let _ = self.child.kill();
    }
}
