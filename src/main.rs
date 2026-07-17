mod cli;

use std::error::Error;
use std::io::{Read, Write};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

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

    // Pump child output -> our stdout, byte for byte, on this (main) thread.
    let mut stdout = std::io::stdout();
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,                       // clean EOF
            Ok(n) => {
                stdout.write_all(&buf[..n])?;
                stdout.flush()?;
            }
            Err(_) => break, // macOS returns EIO (not 0) when the slave closes; treat as EOF
        }
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    Ok(status.exit_code() as i32)
}
