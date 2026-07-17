# Milestone 1 — Launch and Capture (Design Spec)

**Date:** 2026-07-17
**Milestone:** `docs/04-Development/41 - Milestone 1 - Launch and Capture.md`
**Status:** design approved; awaiting spec review before implementation planning.

## 1. Goal

Launch another program from `fuxx-cli` and capture its output while passing it through,
so `fuxx <cmd>` feels identical to running `<cmd>` directly. This is the heart of the
tool and the seam Milestone 2 (OSC detection) will hook into.

## 2. Key decision (differs from the milestone note's "start simple")

The milestone note offers a pipe-first path. During brainstorming we chose to go
**PTY-from-the-start** instead. This is explicitly permitted by the milestone note
("this milestone expands to include a PTY") and by `docs/03-Tech-Stack/33 - Key Crates.md`
(which names `portable-pty`). Reasons:

- A child that detects a **pipe** on stdout (not a TTY) commonly disables colors and,
  critically, may **not emit the OSC notification sequences** that are the entire point
  of fuxx — gating escape output on `isatty()` is common. A PTY makes the child believe
  it is on a real terminal, so it behaves normally and emits those sequences.
- This is how real terminal tools (cmux, Warp, iTerm2, tmux, `script(1)`) all work.
- It also forces **byte-level** reading, which is the shape Milestone 2 needs — so we
  avoid a rewrite. (Line-by-line reading would also *deadlock* on a PTY: an interactive
  prompt like `> ` has no trailing newline, so `.lines()` would buffer it forever and
  the user would never see the prompt.)

**Scope note:** using a PTY means fuxx reads raw bytes *like* a terminal, but it still
does **not** emulate or render one — no VT parser, no screen grid. The wrapper/observer
boundary in `docs/01-Overview/12 - Scope Boundaries.md` and the libghostty rejection in
`docs/02-Architecture/23 - Why Not libghostty.md` both hold unchanged.

## 3. Dependencies (verified on crates.io 2026-07-17; Context7 was unreachable)

- **`portable-pty = "0.9.0"`** — cross-platform PTY interface. Already planned in `33`.
- **`crossterm = "0.29.0"`** — raw-mode control for *our own* terminal. **New addition**
  not yet in `33 - Key Crates`; approved during brainstorming. `portable-pty` manages the
  child's side only; the parent terminal still needs raw mode so keystrokes forward to the
  child immediately instead of being line-buffered/echoed by our shell.
  → Action: add `crossterm` to `33 - Key Crates` during implementation.

## 4. Architecture

Everything lives in `src/main.rs` for this milestone (one small file; split later only if
it grows). Logical units:

1. **Arg parsing** — raw `std::env::args()`. First arg after the binary = program; the
   rest = its arguments. No args → print usage, exit non-zero. (`clap` deferred per `41`.)
2. **PTY setup** — `native_pty_system().openpty(PtySize { rows, cols, pixel_width: 0,
   pixel_height: 0 })`, sized from `crossterm::terminal::size()`. Build the child with
   `CommandBuilder::new(program).args(rest)` and spawn on the **slave**:
   `pair.slave.spawn_command(cmd)` → `Child`.
3. **Raw-mode guard** — `crossterm::terminal::enable_raw_mode()` at start, wrapped in a
   guard struct whose `Drop` calls `disable_raw_mode()`. This guarantees the terminal is
   restored on **every** exit path, including panics and errors. A wedged terminal is the
   worst failure mode here, so restoration must not depend on reaching the happy path.
4. **I/O pump (two threads, byte-level):**
   - **Thread A — child → screen:** read raw byte chunks from
     `pair.master.try_clone_reader()` and `write_all` + `flush` them to our `stdout`. This
     is the pass-through *and* the exact tap point for Milestone 2's OSC scan. Ends on EOF.
   - **Thread B — keyboard → child:** read raw bytes from our `stdin` and write them to
     `pair.master.take_writer()`. Lets us interact with the child.
5. **Exit** — the work happens inside a `run() -> Result<i32>` helper. It calls
   `child.wait()` → `ExitStatus` and **returns** the exit code. The raw-mode guard is a
   local in `run()`, so it drops (restoring the terminal) when `run()` returns — on both
   the `Ok` and `?`-error paths. Only *after* `run()` has returned does `main` call
   `std::process::exit(code)`. See the critical note in §6.

## 5. Data flow

```
args ──▶ CommandBuilder ──▶ spawn on PTY slave ──▶ child (thinks it's a real terminal)
                                                        │
   our stdin ─(Thread B)─▶ master writer ─────────────▶│ (child stdin)
                                                        │
   our stdout ◀─(Thread A)─ master reader ◀────────────┘ (child stdout + stderr, merged)
                                                        │
                                              child exits ──▶ exit_code ──▶ process::exit
```

Note: on a PTY the child's stdout **and** stderr are merged into the single PTY stream, so
we capture both automatically — no separate stderr handling needed.

## 6. Concurrency & lifecycle details (the tricky bits)

- **Drop the slave after spawn.** In `portable-pty`, the master reader only sees EOF once
  all slave handles are closed. We must drop `pair.slave` after `spawn_command` so Thread A
  terminates when the child exits. (Known gotcha — call out in the plan.)
- **Thread B may block on `stdin.read` at exit.** When the child dies, Thread B can be
  parked waiting for a keystroke. For this milestone we do **not** join Thread B; the final
  `process::exit()` tears it down. Acceptable and simple; revisit only if it causes issues.
- **Flush every write** in Thread A so output is not withheld.
- **`std::process::exit()` does NOT run `Drop`.** This is the trap that makes the
  raw-mode guard subtle: if we called `process::exit()` while the guard were still in
  scope, `disable_raw_mode()` would never run and the terminal would be left wedged.
  The fix (see §4.5): all guarded work lives in `run() -> Result<i32>`; the guard drops
  when `run()` returns; `main` calls `process::exit()` only afterwards. The guard still
  covers panics (default unwind runs destructors) and `?`-error returns.

```rust
fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(e)  => { eprintln!("fuxx: {e}"); 1 }
    };
    std::process::exit(code); // guard already dropped inside run()
}
```

## 7. Error handling

- Use `Result` and the `?` operator throughout; return a `Result` from a `run()` helper and
  handle it in `main`.
- Spawn failure (e.g. command not found) → print a clear message to stderr, exit non-zero.
  Do **not** panic, and never leave the terminal in raw mode (the Drop guard covers this).

## 8. Success checks

- `cargo run -- echo hello` → prints `hello`, exits `0`. (spawn + read + exit plumbing)
- `cargo run -- vim` (or `bash`) → behaves like a normal terminal: editing works, colors
  show, keys respond. **This is the real PTY validation** — a classic interactive test
  program, no agent required yet.
- `cargo run -- false` → fuxx exits `1`. Exit-code propagation.
- `cargo run` with no args → usage message + non-zero exit.
- `cargo run -- no-such-command-xyz` → clear error + non-zero exit; terminal not wedged.

## 9. Out of scope / deferred

- **Window-resize handling (SIGWINCH → `master.resize()`).** A refinement, not core.
  Deferred to a small follow-up after M1's core is proven.
- **`clap`** argument parsing — deferred to when real flags exist (per `41`).
- **OSC detection / notifications** — Milestones 2 and 3.
- **Terminal emulation / rendering / libghostty** — out of scope by definition (`12`, `23`).

## 10. What you'll learn (Rust)

`std::thread`, `std::io::{Read, Write}` and byte buffers (`&[u8]`), ownership/borrowing
when moving readers/writers into threads, the `Drop` trait for cleanup guards, and
`Result`/`?` error handling. This is the "where Rust's curve bites" milestone (`41`, `31`).
