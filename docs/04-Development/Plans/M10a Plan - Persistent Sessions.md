---
tags: [development, plan, milestone]
milestone: M10a
status: in-progress
updated: 2026-10-27
---

# M10a Plan — Persistent Sessions (background service, reattach)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[50 - Milestone 10a - Persistent Sessions]] (the spec) · design evidence: [[Input - M10 Feasibility Study]] §2.3 |
| **Status** | 🟨 In progress |
| **Tasks done** | 0 / 12 |
| **▶ Pick up here** | Task 0 |
| **Decisions used** | **Yours (2026-10-27):** opt-in (off by default); notifications while closed; scripting-while-closed later. Design = the study's hybrid daemon (§2.3), AI's call. |
| **Needs your eyes** | 👀 Task 0 (privacy-folder check) and Task 10 |

**Goal:** With `persistent-sessions = on`, a `fuxx daemon` owns PTYs, authoritative `Term`s and status; the GUI is a
client with a replica `Term` per session, fed by a snapshot then numbered output; quitting detaches; reopening
reattaches. Off = today's behaviour, unchanged.

**Architecture:** study §2.3.1 (diagram) and §2.3.2 (module table). That table is binding for file names and
responsibilities. The wire protocol is §2.3.3, the ordering invariant is §2.3.3, lifecycle is §2.3.5, quit and
restore are §2.3.6, and the correctness traps are §2.3.7.

**Tech Stack:** No new crates. `std::os::unix::net`, `posix_spawn` with `POSIX_SPAWN_SETSID` via FFI (as `fonts.rs`
does), serde_json for control payloads, alacritty_terminal 0.26 `Term`/`Processor` on both sides.

**Spec:** [[50 - Milestone 10a - Persistent Sessions]] · [[12 - Scope]]

## Global Constraints

- **Inherited:** everything from M0–M9. **No new crates.**
- **Gates:** 0 warnings; `cargo fmt --check`; every test passes (start: 243).
- **Off is unchanged:** with `persistent-sessions` off (the default), no daemon is started or contacted, and every existing behaviour and test is untouched. `TerminalCore` keeps a `Local` backend for this.
- **GPUI-free daemon:** `daemon*.rs`, `mux_proto.rs`, `vt_snapshot.rs` import nothing from `gpui`. Neither do `pty.rs`, `status.rs` and `detector.rs`, which already don't.
- **Socket:** `~/Library/Application Support/fuxx/fuxxd.sock`. It reuses M9's rules: 0700 dir, 0600 socket, `getpeereid`, a connection cap, bounded queues, never replacing a live socket.
- **Protocol:** study §2.3.3.
  - Frame = `u32 BE length | u8 kind | payload`, length ≤ 16 MiB.
  - `Input`/`Output` carry a binary header (`session u64`, `seq u64`) followed by raw bytes. Other kinds carry JSON with additive fields; unknown fields and unknown kinds are ignored.
  - `Hello` negotiates `PROTO_MIN..=PROTO_MAX`, both 1 now.
- **Ordering invariant:** advancing the authoritative Term, bumping `seq` and queueing to subscribers happen under one lock. `Attach` snapshots and subscribes under the same lock.
- **Ids:** the daemon owns `s<N>` and never reuses them while it lives.
- **Env:** `Create` carries the GUI's environment, including `FUXX_*` and the GUI exe's folder in `PATH`. The daemon never uses its own frozen env for children.
- **Security:** control flows only over the socket, never in-band from PTY output.
- **Lifecycle:** the GUI spawns the daemon on demand (`current_exe() daemon`, setsid, log at `~/Library/Logs/fuxx/daemon.log`). The daemon exits 60 s after it has no sessions and no clients.
- **No `cargo run` by subagents.** GUI behaviour is checked by the user (Tasks 0, 10). The daemon is tested in-process against temp sockets.

## Review Focus

1. **A reattached screen differs from what was there** (wrapped lines, wide chars, alt screen, modes, cursor) → `vt_snapshot` round-trip tests on recorded streams (Task 1).
2. **Output lost or duplicated around attach or resize** → the ordering-invariant test, plus resize applied at its `seq` (Tasks 3, 5).
3. **A program hangs while the window is closed** because nobody answers DA, cursor or colour queries → the daemon answers (Task 3 test with a child that queries).
4. **Sessions leak or vanish** (the daemon lingers forever, or dies with the GUI) → setsid spawn, the idle-exit test, and the 👀 `kill -9` check (Tasks 4, 10).
5. **Turning it off changes anything** → every existing test runs unchanged; Local backend by default (Task 5).

## Files

As study §2.3.2: `vt_snapshot.rs`, `mux_proto.rs`, `daemon.rs`, `daemon_session.rs`, `daemon_client.rs` (new);
`terminal.rs`, `terminal_view.rs`, `workspace.rs`, `restore.rs`, `workspace_api.rs`, `cli.rs`, `main.rs`,
`appearance.rs`, `preferences.rs`, `notifier.rs` (changed).

---

### Task 0: Spike — does a process that outlives fuxx keep folder access? — ⬜

**Throwaway**, removed at the end of the milestone or kept only if it becomes the daemon's spawn code.

- A hidden CLI command `fuxx __detach-probe`. It spawns `current_exe() __detach-probe-child` with `posix_spawn` + `POSIX_SPAWN_SETSID`, and stdio sent to `~/Library/Logs/fuxx/probe.log`.
- The child waits 15 s, then logs:
  - `ls ~/Documents`, `ls ~/Desktop` and `ls ~/Downloads` (counts or errors);
  - touching and removing a file in each;
  - its ppid.
- A second variant, `fuxx __detach-probe --disclaim`, also calls `responsibility_spawnattrs_setdisclaim` (private API, looked up with `dlsym`; skipped with a log line if absent).
- `scripts/bundle-mac.sh --app-only` builds just `target/dist/fuxx.app`: no DMG, no version check.
- **👀 You:**
  1. Open that `fuxx.app` from Finder.
  2. In a session, run `fuxx __detach-probe`, then quit fuxx within 15 s.
  3. A minute later, read `~/Library/Logs/fuxx/probe.log`.
  4. Repeat with `--disclaim`.
- The result decides the daemon's spawn mode, or documents the limit (spec "Limits"). It gets recorded in the milestone note.

### Task 1: `vt_snapshot` — a Term as bytes that rebuild it — ⬜

**Files:** Create `src/vt_snapshot.rs` (pure, no gpui).

**Interface:** `pub fn encode<L: EventListener>(term: &Term<L>) -> Vec<u8>`. Contents per study §2.3.2:
- a reset first;
- scrollback and screen with SGR runs;
- wide and zero-width characters;
- soft wraps emitted as overflow, never `\r\n`;
- OSC 8 hyperlinks;
- the alt screen: primary contents first, then `CSI ?1049h` and the alt contents;
- the scroll region;
- modes: DECCKM, bracketed paste, mouse modes and encoding, focus reporting, origin, autowrap, kitty keyboard flags;
- the title;
- the cursor position, shape and visibility, last.

**Tests (TDD).** Helper `round_trip(bytes, cols, rows)`:
1. feed into Term A;
2. `encode(A)`;
3. feed into a fresh Term B of the same size;
4. assert equal: every grid cell (char, fg, bg, flags) across history and screen, the cursor, mode flags, and the title.

Cases, each its own test:
- plain coloured text;
- a line wrapped across rows, then a resize narrower and back (B reflows like A);
- wide chars and combining marks;
- alt screen active with primary content underneath;
- scroll region set;
- bracketed paste + app cursor + mouse SGR modes;
- an OSC 8 link;
- a long history (500 lines);
- a recorded fragment of a Claude Code redraw (synthesised with cursor moves, erase-line and SGR).

### Task 2: `mux_proto` — frames and messages — ⬜

**Files:** Create `src/mux_proto.rs` (pure).

**Interface:**
- A `Frame` enum covering every study §2.3.3 message in both directions.
- `encode(&Frame) -> Vec<u8>`.
- `Decoder` (incremental: `push(&[u8])`, `next() -> Option<Result<Frame, ProtoError>>`).
- `PROTO_MIN`/`PROTO_MAX`, and `negotiate(client_min, client_max) -> Result<u32, ProtoError>`.
- `MAX_FRAME` = 16 MiB.

**Tests:**
- every frame kind round-trips;
- a frame split across many `push` calls decodes once;
- two frames in one push;
- an oversize length → error;
- an unknown kind → `Frame::Unknown` (ignored);
- JSON with an extra field decodes;
- negotiation: overlap picks the max common version, none → `Incompatible`.

### Task 3: `daemon_session` — one session owned by the daemon — ⬜

**Files:** Create `src/daemon_session.rs`. Move the PTY/Term/reader/writer/detector/tracker logic from `terminal.rs`; `terminal.rs` keeps a thin Local backend reusing it.

**Interface:**
- `DaemonSession::spawn(spec: CreateSpec) -> io::Result<Self>`. `CreateSpec` has cwd, startup, env, dims, cell px, name, project, preset.
- `attach(&self, sub: Subscriber) -> SnapshotFrame`, under the lock, per the invariant.
- `detach(sub_id)`, `input(bytes)` (calls `tracker.on_input`), `resize(cols, rows, cell)` (emits `Resized` into the seq), `set_theme`, `set_meta`, `close()`.
- `tick(now)` for status and `flush_stale_sync`.
- `answer_queries`: DA, DSR/CPR, OSC 4/10/11/12 from the last theme, CSI 14t from the last cell px.
- Events go out as frames: Status, Title, Bell, Exited.
- Each subscriber has a bounded queue; overflow → drop its backlog + send `Resync`.

**Tests (TDD, using `sh -c` children as `terminal.rs` tests do):**
- output arrives in seq order;
- attach-while-streaming gives a snapshot at N plus frames from N+1 with no gap or overlap, comparing a replica Term fed by both against the authoritative one;
- a child sending `\x1b[6n` gets a reply with no client attached;
- resize appears in the stream at the right seq;
- an overflowing subscriber gets `Resync`;
- the status goes Working, then back once quiet.

### Task 4: `daemon` — the service and its commands — ⬜

**Files:** Create `src/daemon.rs` · Modify `src/cli.rs`, `src/main.rs`.

**Interface:**
- `fuxx daemon [run]` runs in the foreground.
- The socket uses M9's patterns (factor shared helpers out of `api_server.rs` rather than copying).
- `Hello` handshake.
- A session table with ids `s<N>`.
- One 50 ms tick thread.
- One UI client holds resize authority at a time; a new UI attach takes over.
- Idle exit after 60 s with no sessions and no clients.
- `fuxx daemon status` prints pid, version, protocol, uptime and the session count, or "not running" with exit 3.
- `fuxx daemon stop [--force]` refuses with the session names unless `--force`, which SIGHUPs each process group.
- A crash log to `~/Library/Logs/fuxx/daemon.log`.

**Tests:**
- in-process daemon on a temp socket: handshake OK and incompatible;
- Create / List / Close;
- idle exit (with a short timeout injected);
- stop refused while sessions exist, then `--force` works;
- CLI parse of `daemon status|stop|run`.

### Task 5: The GUI as a client — ⬜

**Files:** Create `src/daemon_client.rs` · Modify `src/terminal.rs`, `src/terminal_view.rs`.

**Interface:**
- `DaemonClient::connect_or_spawn(paths) -> io::Result<Self>`: spawns with setsid (Task 0's result decides `--disclaim`), retrying up to 2 s.
- One reader thread demuxes frames into per-session channels; one bounded writer.
- `TerminalCore` gets `enum Backend { Local(..), Daemon { id, client } }`. In Daemon mode:
  - it starts from `Snapshot`;
  - it feeds the replica from `Output`;
  - it applies `Resized` at its seq;
  - `Resync` → re-`Attach`;
  - input and resize go to the daemon;
  - the replica's PtyWrite, ColorRequest and TextAreaSizeRequest are ignored (the daemon answers);
  - Status, Title and Exited come from frames.
- `TerminalView` takes status from the backend in Daemon mode (it doesn't run its own tracker).

**Tests:** in-process daemon + client.
- Type `echo hi\r` and the replica shows `hi`.
- Attach to a busy session matches the daemon's screen.
- Resize round-trips.
- Local backend tests are untouched.

### Task 6: Workspace — create, close, quit, ids — ⬜

**Files:** Modify `src/workspace.rs`, `src/workspace_api.rs`, `src/main.rs`.

**Interface (only when persistence is on):**
- `open_session` → `client.create(...)`, and the id comes from the daemon.
- `close_at` → `Close`.
- ⌘Q / the red button → detach (no `Close`).
- New menu item **Quit fuxx and End All Sessions** (`alt-cmd-q`) → `Close` all, then quit.
- When N > 0 sessions are live, the fuxx menu's Quit item reads "Quit fuxx (N sessions keep running)". If GPUI can't retitle a menu item at runtime, keep the plain title and add the count as a disabled info item.
- M9 handlers keep working, because sessions are daemon-backed.
- Scripting env vars come from the GUI, as today.

**Tests:** pure helpers only; the rest is checked in 👀.

### Task 7: Reopen — adopt live sessions, restore the rest — ⬜

**Files:** Modify `src/workspace.rs`, `src/restore.rs`.

**Interface:**
- `SavedSession` gains `#[serde(default)] daemon_id: Option<u64>`; VERSION stays 1.
- At startup with persistence on:
  - `List` → live sessions;
  - pure `place(saved: &SavedState, live: &[LiveInfo]) -> Placement`, which:
    - matches by `daemon_id`;
    - keeps groups, layouts, collapsed and active;
    - lists saved-but-dead sessions for M7 restore per `RestoreMode`;
    - appends live-but-unknown sessions as their own layouts.
- **Lazy attach:** only on-screen sessions attach at once; the others attach on first view, or in the background after 1 s.

**Tests (pure `place`):**
- all live and matched;
- one dead, which goes to restore;
- one unknown, which is appended;
- a layout with a dead pane is sanitized (M8 `sanitize`);
- the active session is kept or falls back.

### Task 8: The setting and the Preferences row — ⬜

**Files:** Modify `src/appearance.rs`, `src/preferences.rs`.

- `persistent-sessions = on|off` (default off; warns on other values), read at startup.
- Preferences → **Sessions → Keep running after quit**: On/Off, saved like the other rows. The note says "takes effect next launch", and adds the privacy-folder limit if Task 0 found one.

**Tests:** `from_entries` parsing (default, on, off, invalid → warning).

### Task 9: Notifications while the window is closed — ⬜

**Files:** Modify `src/daemon.rs` (+ `notifier.rs` reuse).

- When **no UI client is attached**, the daemon fires the same notifications as fuxx does today on status changes to Waiting or Done, using `notifier::should_notify` with `is_active = false` and `window_active = false`.
- With a UI attached, the GUI keeps doing it, focus-aware.

**Tests:** the pure decision (attached / not attached × status change).

### Task 10: On screen — 👀 ⬜

With `persistent-sessions = on` and a restart:

- [ ] The success check from the spec: two claude sessions (one split) → ⌘Q → a question arrives and a notification appears → reopen: scrollback, layout, waiting ring, answer works.
- [ ] The same after `kill -9` of fuxx (`pkill -9 -f "target/debug/fuxx$"` is not enough; use the pid from `fuxx daemon status` → the GUI's pid).
- [ ] vim / htop / Claude Code look right after reattach. A resize while attached still re-fits.
- [ ] ⌥⌘Q ends everything. `fuxx daemon status` then says not running, after about 60 s.
- [ ] `fuxx daemon stop` refuses while sessions run; `--force` ends them.
- [ ] Turning the setting off and restarting gives exactly the old behaviour.
- [ ] M9 commands (`fuxx ls/send/wait/answer`) work against reattached sessions, and `$FUXX_SESSION` still points at the right one.

### Task 11: Update the docs — ⬜

- [ ] Milestone note 50 ✅ + Task 0 result; plan ✅; roadmap; tracker; Progress Log; STRUCTURE.md; README (one line about persistent sessions).

## Verification log

_(filled in as tasks complete)_

## Known limitations

- Sessions end at logout or reboot (M7 restore brings the layout back).
- Scripting needs the window (later step).
- Upgrading fuxx doesn't carry sessions into a new background service if the protocol changes.

## Related

- [[50 - Milestone 10a - Persistent Sessions]] · [[Input - M10 Feasibility Study]] · [[40 - Roadmap]] · [[00 - Plan Tracker (Start Here)]]
