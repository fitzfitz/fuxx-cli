mod cli;
mod detector;
mod notifier;

use std::error::Error;
use std::io::{Read, Write, IsTerminal};

use detector::OscDetector;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

/// Puts the terminal in raw mode (only if we're actually interactive) and guarantees it is
/// restored on every exit path — including `?`-errors and panics — because `Drop` runs then.
/// Note: `std::process::exit` does NOT run `Drop`, which is why `main` calls it only *after*
/// `run` (and therefore this guard) has returned.
struct RawModeGuard {
    active: bool,
}

impl RawModeGuard {
    fn new() -> Result<Self, Box<dyn Error>> {
        if std::io::stdin().is_terminal() {
            crossterm::terminal::enable_raw_mode()?;
            Ok(Self { active: true })
        } else {
            Ok(Self { active: false })
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fuxx: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32, Box<dyn Error>> {
    let launch = cli::parse(std::env::args())?;

    // Size the PTY to our terminal if we can; fall back to a sane default otherwise.
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let _guard = RawModeGuard::new()?; // restored when `run` returns (see struct docs)

    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?; // portable-pty errors -> our boxed error

    let mut cmd = CommandBuilder::new(&launch.program);
    cmd.args(&launch.args);
    let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;

    // Drop the slave now that the child holds it: the master reader only reaches EOF
    // once every slave handle is closed.
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    // Copy our stdin -> the child, on a background thread. Detached on purpose: it may block
    // in `read` waiting for a keystroke when the child exits; the final process::exit tears it
    // down. `take_writer` hands us the master's write side.
    let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 1024];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if writer.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    let _ = writer.flush();
                }
            }
        }
    });

    // Pump child output -> our stdout, byte for byte, on this (main) thread.
    let mut stdout = std::io::stdout();
    let mut buf = [0u8; 8192];
    let mut detector = OscDetector::new();
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,                       // clean EOF
            Ok(n) => {
                stdout.write_all(&buf[..n])?;
                stdout.flush()?;
                if detector.feed(&buf[..n]) > 0 {
                    notifier::fire("fuxx-cli", "Your agent sent a notification");
                }
            }
            Err(_) => break, // macOS returns EIO (not 0) when the slave closes; treat as EOF
        }
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    Ok(status.exit_code() as i32)
}
