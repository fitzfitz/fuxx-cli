# Milestone 1 — Launch and Capture — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `fuxx <command> [args...]` launch the command on a PTY, pass its output and your keystrokes through transparently, and exit with the child's exit code.

**Architecture:** A single binary (`src/main.rs`) plus a small pure arg-parsing module (`src/cli.rs`). We spawn the child on a pseudo-terminal (`portable-pty`) so it behaves as if attached to a real terminal. Output is pumped byte-for-byte from the PTY master to our stdout on the main thread; our stdin is copied to the PTY master on one background thread. The terminal is put in raw mode (only when we're actually interactive) and restored via a `Drop` guard, with `process::exit` called only after that guard has dropped.

**Tech Stack:** Rust (edition 2024), `portable-pty = "0.9.0"`, `crossterm = "0.29.0"`, std threads + `std::io`.

## Global Constraints

- Rust **edition 2024**; crate lives at the **repo root** (alongside `docs/`).
- Dependencies (verified on crates.io 2026-07-17; Context7 was unreachable):
  `portable-pty = "0.9.0"`, `crossterm = "0.29.0"`. No other runtime deps.
- **macOS-first.** Single small native binary.
- **Wrapper/observer model only** — no terminal emulation, no rendering, no libghostty
  (`docs/01-Overview/12 - Scope Boundaries.md`, `docs/02-Architecture/23 - Why Not libghostty.md`).
- **Byte-level** I/O (`&[u8]`), never line-oriented (an interactive prompt has no trailing newline).
- Beginner-facing: explain each Rust concept in the commit/PR notes; every success check must
  be run and observed, not assumed (`docs/05-Agentic-Workflow/51 - Delegation Principles.md`).

## Refinements to the approved spec (design unchanged in substance)

- Spec §4/§6 said "two threads." Implementation uses **one** background thread (stdin→child);
  output is pumped on the main thread. Same concurrency, simpler, and it lets the main thread
  drain all output before exit with no join race.
- Raw mode is **gated on `std::io::stdin().is_terminal()`** so non-interactive/piped runs (incl.
  the automated tests) don't attempt raw mode.
- The output loop treats **any read error on the PTY master as end-of-stream** — macOS returns
  `EIO` rather than a clean `0`-byte EOF when the child's slave side closes.

---

### Task 1: Argument parsing (pure, unit-tested)

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `struct Launch { program: String, args: Vec<String> }` and
  `fn parse<I: IntoIterator<Item = String>>(raw: I) -> Result<Launch, String>`.
  `parse` skips `argv[0]`, takes the next item as `program`, the rest as `args`, and returns
  `Err(usage)` when no command is given. Task 2 consumes `Launch`.

- [ ] **Step 1: Write the failing tests** in `src/cli.rs`

```rust
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
}

pub fn parse<I: IntoIterator<Item = String>>(raw: I) -> Result<Launch, String> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_program_and_args() {
        let launch = parse(["fuxx", "echo", "hello"].map(String::from)).unwrap();
        assert_eq!(launch.program, "echo");
        assert_eq!(launch.args, vec!["hello".to_string()]);
    }

    #[test]
    fn no_command_is_usage_error() {
        let err = parse(["fuxx"].map(String::from)).unwrap_err();
        assert!(err.contains("usage"), "message was: {err:?}");
    }
}
```

- [ ] **Step 2: Wire the module and run the tests to see them fail**

Add to the top of `src/main.rs`:

```rust
mod cli;

fn main() {
    match cli::parse(std::env::args()) {
        Ok(launch) => println!("would launch: {} {:?}", launch.program, launch.args),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
```

Run: `cargo test`
Expected: compiles, tests FAIL at `unimplemented!()` (panic in `parse`).

- [ ] **Step 3: Implement `parse`** — replace the `unimplemented!()` body in `src/cli.rs`

```rust
pub fn parse<I: IntoIterator<Item = String>>(raw: I) -> Result<Launch, String> {
    let mut it = raw.into_iter();
    it.next(); // skip argv[0] (the fuxx binary path)
    let program = it
        .next()
        .ok_or_else(|| "usage: fuxx <command> [args...]".to_string())?;
    let args: Vec<String> = it.collect();
    Ok(Launch { program, args })
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test`
Expected: PASS (2 passed). Also `cargo run -- echo hi` prints `would launch: echo ["hi"]`,
and `cargo run` prints the usage error and exits non-zero.

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat(m1): parse the wrapped command and its args"
```

---

### Task 2: Spawn on a PTY, pass output through, propagate exit code

**Files:**
- Modify: `Cargo.toml` (add the two dependencies)
- Modify: `src/main.rs` (replace `main`, add `run`)
- Create: `tests/wrapper.rs` (integration tests)

**Interfaces:**
- Consumes: `cli::parse` → `Launch` from Task 1.
- Produces: `fn run() -> Result<i32, Box<dyn std::error::Error>>` returning the child's exit
  code; `main` maps errors to exit code 1 and calls `std::process::exit`. Task 3 adds stdin
  forwarding and the raw-mode guard inside `run`.

- [ ] **Step 1: Write the failing integration tests** in `tests/wrapper.rs`

`CARGO_BIN_EXE_fuxx-cli` is an env var Cargo sets for integration tests, pointing at the built
binary. These run fuxx with piped (non-terminal) stdin/stdout, which is why raw mode must stay
off in that case (added in Task 3).

```rust
use std::process::Command;

fn fuxx() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fuxx-cli"))
}

#[test]
fn echo_is_passed_through() {
    let out = fuxx().args(["echo", "hello"]).output().unwrap();
    assert!(out.status.success(), "status: {:?}", out.status);
    let s = String::from_utf8_lossy(&out.stdout);
    // A PTY translates "\n" to "\r\n", so match on the substring, not equality.
    assert!(s.contains("hello"), "stdout was: {s:?}");
}

#[test]
fn exit_code_is_propagated() {
    let status = fuxx().arg("false").status().unwrap();
    assert_eq!(status.code(), Some(1));
}

#[test]
fn no_args_is_an_error() {
    let out = fuxx().output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn unknown_command_is_an_error() {
    let out = fuxx().arg("definitely-not-a-real-command-xyz").output().unwrap();
    assert!(!out.status.success());
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test --test wrapper`
Expected: `echo_is_passed_through` FAILS (current `main` prints `would launch: ...`, not `hello`);
`exit_code_is_propagated` FAILS (fuxx exits 0). This confirms the tests exercise real behavior.

- [ ] **Step 3: Add dependencies**

Run:

```bash
cargo add portable-pty@0.9.0
cargo add crossterm@0.29.0
```

Expected: `Cargo.toml` `[dependencies]` now lists `portable-pty = "0.9.0"` and `crossterm = "0.29.0"`.

- [ ] **Step 4: Implement `run` (output pump + exit code)** — replace all of `src/main.rs`

Stdin forwarding and raw mode arrive in Task 3; this step is enough to pass Task 2's tests.

```rust
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test`
Expected: PASS — Task 1's unit tests plus all four `wrapper` tests. Also try it live:
`cargo run -- echo hello` prints `hello`; `cargo run -- ls` shows a directory listing.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs tests/wrapper.rs
git commit -m "feat(m1): spawn command on a PTY, pass output through, propagate exit code"
```

---

### Task 3: Forward keystrokes + raw mode (interactive), restored safely

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `run` from Task 2.
- Produces: a `RawModeGuard` whose `Drop` restores the terminal; a background thread copying
  stdin → PTY master. No new public functions.

This task's core is interactive, so its primary check is **manual** (running a real TUI program
through fuxx). The automated tests from Task 2 must keep passing as a regression guard.

- [ ] **Step 1: Add the raw-mode guard and stdin-forwarding thread** — edit `src/main.rs`

Add the import and guard near the top (after the existing `use` lines):

```rust
use std::io::IsTerminal;

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
```

In `run`, enable the guard right after computing the size, and start the stdin thread right
after `try_clone_reader`. Insert the guard line:

```rust
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let _guard = RawModeGuard::new()?; // restored when `run` returns (see struct docs)
```

And after `let mut reader = pair.master.try_clone_reader()...;` add:

```rust
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
```

- [ ] **Step 2: Run the automated tests (regression)**

Run: `cargo test`
Expected: PASS — all Task 1 + Task 2 tests still green (piped stdin ⇒ guard inactive ⇒ raw mode
never touched).

- [ ] **Step 3: Manual interactive verification** (the real point of the PTY)

Run each and observe:

- `cargo run -- vim` — vim opens, renders with color, arrow keys and `:q!` work, and on quit your
  shell prompt is normal (terminal restored, no stray raw mode). vim is the classic PTY test.
- `cargo run -- bash` — you get an interactive shell; run `ls --color=auto` and see colors
  (proves the child sees a TTY); `exit` returns you cleanly.
- Press `Ctrl-C` while `cargo run -- bash` runs a `sleep 100` — the signal reaches the child.

Expected: all behave as if run directly. If the terminal is ever left in raw mode after exit
(no echo, `Enter` doesn't work), run `reset` and treat it as a bug in the guard.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(m1): forward stdin and enable raw mode with a restore guard"
```

---

### Task 4: Verify the milestone and update the vault

**Files:**
- Modify: `docs/03-Tech-Stack/33 - Key Crates.md` (record `crossterm`)
- Modify: `docs/04-Development/41 - Milestone 1 - Launch and Capture.md` (status + checklist)
- Modify: `docs/04-Development/40 - Development Roadmap.md` (Milestone 1 ✅)
- Modify: `docs/06-Progress-Log/60 - Progress Log.md` (new entry + snapshot)

Doc updates are required by AGENTS.md Rule 4 and `docs/05-Agentic-Workflow/54 - Session Startup Checklist.md`.

- [ ] **Step 1: Re-run the full milestone success checks**

Run: `cargo test` (all green) and re-confirm `cargo run -- echo hello`, `cargo run -- false`
(exit 1 via `echo $?`), and the manual `cargo run -- vim` check from Task 3. Only proceed once
you have observed each pass.

- [ ] **Step 2: Record the `crossterm` dependency** in `docs/03-Tech-Stack/33 - Key Crates.md`

Add a section after the `notify-rust` section:

```markdown
## crossterm — raw-mode control for our own terminal

`portable-pty` manages the child's terminal; our own terminal still needs raw mode so keystrokes
forward to the child instantly instead of being line-buffered/echoed by our shell. `crossterm`'s
`enable_raw_mode` / `disable_raw_mode` handle this. Added in Milestone 1. Also the likely home for
the deferred window-resize handling.
```

- [ ] **Step 3: Mark Milestone 1 done** in `docs/04-Development/41 - Milestone 1 - Launch and Capture.md`

Set `status: done` in frontmatter, `**Status:: ✅ Done**`, and check every checklist box. Add a
one-line note that stdin forwarding + raw mode were included and SIGWINCH resize was deferred.

- [ ] **Step 4: Flip the roadmap** in `docs/04-Development/40 - Development Roadmap.md`

Change the Milestone 1 line from `⬜` to `✅`.

- [ ] **Step 5: Add a Progress Log entry** at the top of the Entries section in
`docs/06-Progress-Log/60 - Progress Log.md`, and update the status snapshot to point at
`[[42 - Milestone 2 - Detect the Signal]]`. Use the template in `61 - Log Entry Template`
(Did / Learned / Blocked / Next task).

- [ ] **Step 6: Commit**

```bash
git add "docs/03-Tech-Stack/33 - Key Crates.md" "docs/04-Development/41 - Milestone 1 - Launch and Capture.md" "docs/04-Development/40 - Development Roadmap.md" "docs/06-Progress-Log/60 - Progress Log.md"
git commit -m "docs(m1): record crossterm dep, mark Milestone 1 done, log progress"
```

---

## Self-Review (author's check against the spec)

**Spec coverage:**
- Args (§4.1) → Task 1. PTY setup (§4.2) → Task 2. Raw-mode guard (§4.3) → Task 3. I/O pump
  (§4.4) → output Task 2 / input Task 3. Exit code (§4.5) → Task 2. Error handling (§7) → Task 2
  `main`/`run` + Task 3 guard. Success checks (§8) → Task 2 automated + Task 3 manual. Deferred
  items (§9) → left undone by design; recorded in Task 4. Dependency recording (§3) → Task 4.
  All spec sections map to a task.

**Placeholder scan:** No TBD/TODO. The only `unimplemented!()` is a deliberate red-test step in
Task 1 and is replaced in the same task. All code steps show complete code.

**Type consistency:** `Launch { program, args }` and `parse` signature identical across Task 1
and its use in Task 2. `run() -> Result<i32, Box<dyn Error>>` consistent across Tasks 2–3.
`RawModeGuard` methods consistent. `Box<dyn Error>` is the single error type; portable-pty errors
are converted with `.map_err(|e| e.to_string())` everywhere they occur.
