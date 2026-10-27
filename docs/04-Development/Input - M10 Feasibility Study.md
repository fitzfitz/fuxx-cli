---
tags: [development, input, research]
---

> [!note] Input for M10 (2026-10-27), written by a researcher agent. Decisions taken from it are recorded in [[40 - Roadmap]] and the M10a note; this file is the evidence.

# M10 feasibility study: persistent sessions, SSH workspaces, in-app browser

*Written 2026-10-27 by a research/architecture agent. Read-only: nothing in the repo was changed, and the GUI was
not run. This is input for a decision, not a plan: under AGENTS.md rule 3, nothing here is in scope until the owner
changes [[12 - Scope]] and [[40 - Roadmap]] (both still list these three under "Out of scope (for now)").*

Conventions:
- **(verified)** means checked here, against the pinned source in `~/.cargo/git/checkouts/zed-a70e2ad075855582/1d2a4b3`,
  `~/.cargo/registry/src/*/alacritty_terminal-0.26.0`, the fuxx repo, or an official doc page fetched today.
- **(unverified)** means a single third-party source, a summary from a search engine, or my own knowledge that I
  didn't re-check today.
- Sizes are relative to fuxx's own milestones. M8 (split panes) is about 740 lines in `split_tree.rs` + `panes.rs`
  plus workspace changes. M9 (scripting) is about 4,970 lines across `api*.rs`, `cli.rs`, `mcp.rs` and
  `workspace_api.rs`. **S** is under a third of M8, **M** is about one M8, **L** is about one M9, and **XL** is more
  than one M9.

---

## 0. Summary

| Part | Recommendation | Size | Confidence |
|---|---|---|---|
| **M10a persistent sessions** | A `fuxx daemon` (same binary, one process, no GPUI) owns each PTY **and an authoritative headless alacritty `Term`**, plus the OSC detector and the status tracker. The GUI keeps its own **replica `Term`** per session. On attach, the daemon sends the replica a **VT-encoded snapshot** (scrollback, screen, modes, cursor). After that it forwards raw output bytes in order, each with a sequence number. Rendering, selection, links and scrollback stay local and unchanged. The daemon answers terminal queries, keeps status, and assigns stable session ids. M7's restore stays as the cold path for after a reboot. M9's socket stays in the GUI for M10a. | **L** (≈ one M9, 3.5–5k lines incl. tests) | Medium-high. Two risks need spikes first: TCC attribution once the GUI has quit, and snapshot fidelity |
| **M10b SSH workspaces** | **Option A**: an SSH session is a fuxx session running local `ssh` (Zed's remote terminals work this way too). It has a host picker from `~/.ssh/config` / `ssh -G`, a host group in the sidebar, a reconnect prompt, and optional `tmux new -A` on the remote for remote persistence. Status detection works unchanged because it reads the byte stream. With M10a, the ssh *client* also survives GUI restarts. **Option B** (a remote fuxx daemon) is deferred, but M10a's protocol is kept transport-agnostic so B stays possible. | **A: S–M** · B: XL | High for A |
| **M10c in-app browser** | It is technically feasible at rev 1d2a4b3 (GPUI exposes `HasWindowHandle`; a `WKWebView` can be added as a native subview via `wry::build_as_child`). It is **not recommended as an in-pane browser now**: z-order, focus, key-equivalent and clipping problems against GPUI's single Metal view, plus little value over Playwright/Chrome DevTools MCP for agents. If the owner wants something, do **(c) a separate browser window per session**, or nothing. | In-pane: **L**, high uncertainty · separate window: **M** · nothing: 0 | Medium |

**Order:** keep M10a → M10b → M10c. One change is worth considering: **M10b Option A doesn't depend on M10a** and is
small, so it could go first as a quick win. If M10a lands first, ssh sessions get "survives GUI quit" for free. The
order matters only because M10a must be built GPUI-free and transport-agnostic so that M10b Option B can reuse it.
Section 5 has the details.

---

## 1. What fuxx has today (the facts the design rests on)

All of these were verified in the repo at `165cb8d`.

- `src/terminal.rs` `TerminalCore`:
  - It holds `Arc<FairMutex<Shared{term, processor}>>`, a `Pty`, dims, an input `mpsc::Sender`, a `LinkFinder`
    and a theme.
  - `spawn` starts a **reader thread** (`fuxx-pty-reader`). For each read it runs `OscDetector::feed`, then
    `processor.advance(term, bytes)` under the lock, then sends `TermEvent::Output{notifications}`.
  - It also starts a **writer thread** (`fuxx-pty-writer`) that drains the input channel, so a stuck child can't
    block the UI.
  - `flush_stale_sync` is called on a 50 ms ticker to un-stick DEC 2026 synchronized updates.
- `src/terminal_view.rs`:
  - The **query replies are produced in the GUI**: `Event::PtyWrite`, `ColorRequest` (needs the theme) and
    `TextAreaSizeRequest` (needs the cell pixel size) are written back to the PTY from `on_term_event`.
  - `StatusTracker` lives in the view. It is fed by `Output`, `Bell`, input timestamps and a 50 ms tick that reads
    `tail_text(8)`.
- `src/status.rs` and `src/detector.rs` are **pure and GPUI-free**, so they can move to a daemon unchanged.
- `src/pty.rs`: `Drop for Pty` kills the child, so today a GUI exit or crash ends every session. The PTY master fd
  closing also hangs up the shells.
- `src/workspace.rs`:
  - `Session{id, view, status, title, name, project, preset, note, progress}`.
  - `next_id` gives **ids that are fresh each launch** (`s1`, `s2`, …).
  - `open_session` sets `FUXX_SOCKET`/`FUXX_SESSION` in each shell's environment (`scripting_env`).
  - `save()` writes `sessions.json`: name, project, preset, layouts, collapsed groups, active.
- `src/restore.rs`: `RestoreMode{Off, Layout, Agents}`. "Agents" re-runs the preset command in a new shell. This is
  a cold restart, not a reattach.
- `src/api_server.rs`: the M9 socket at `~/Library/Application Support/fuxx/fuxx.sock`, mode 0600 in a 0700
  folder, with a `getpeereid` same-user check, a bounded per-connection write queue, at most 64 connections and
  16 MiB lines. These patterns can be reused for a daemon socket.
- alacritty_terminal 0.26.0:
  - Its `serde` feature is **on by default**. `Grid<T>` is `Serialize`, but `cursor` and `saved_cursor` are
    `#[serde(skip)]` (`grid/mod.rs:109-117`), and `Term` itself is not serializable: modes, the inactive grid and
    the keyboard-mode stack are private.
  - `Term::damage()` / `reset_damage()` exist (`term/mod.rs:458,489`).
  - The default `scrolling_history` is 10,000 lines (`term/mod.rs:359`). fuxx uses `Config::default()`.

---

## 2. M10a: persistent sessions

### 2.1 How others do it

| Tool | Who owns the PTY | Who owns screen state | On reattach | Notes |
|---|---|---|---|---|
| **tmux** | Server process | Server (full grid + history per pane) | The server **redraws the visible screen** to the client terminal. History stays server-side (copy-mode). | Client and server are separate processes over a socket in /tmp ([man tmux](https://man.freebsd.org/cgi/man.cgi?query=tmux), verified). A local tmux server dies at logout and reboot. |
| **tmux control mode (`-CC`, used by iTerm2)** | tmux server | **Both**: tmux keeps its grid, and the client (iTerm2) runs **its own emulator** fed by `%output <pane> <octal-escaped raw bytes>` | The client fetches contents with `capture-pane`. With flow control (`pause-after`), a lagging pane is paused, "and it is up to the client to update the content of the pane … using capture-pane". | [Control Mode wiki](https://github.com/tmux/tmux/wiki/Control-Mode) (verified). **This is the closest precedent to the recommended hybrid.** That iTerm2 uses `capture-pane -e` for history is my recollection (unverified). |
| **zellij** | Server | Server | The server sends render instructions; the client is thin. Resurrection after a quit or crash re-creates the layout and commands (opt-in viewport/scrollback serialization). Commands wait behind "Press ENTER to run". | [Session resurrection](https://zellij.dev/documentation/session-resurrection) (search summary, unverified detail). Like fuxx M7, resurrection does not keep processes alive. |
| **WezTerm mux** (`wezterm-mux-server`, `wezterm connect unix`, SSH `SSHMUX:` domains) | Mux server | Server. The client **pulls render changes and lines** via PDUs (`GetPaneRenderChanges`, `GetLines`, `SendKeyDown`, `SendPaste`, `SendMouseEvent`, `Resize`, `WriteToPane`). | The client re-fetches lines. Optional predictive local echo (`local_echo_threshold_ms`) hides the round-trip latency. | [Multiplexing docs](https://wezterm.org/multiplexing.html) (verified). PDU names and `CODEC_VERSION = 46` "must be bumped when backwards incompatible changes are made" ([codec/src/lib.rs](https://raw.githubusercontent.com/wezterm/wezterm/main/codec/src/lib.rs), verified). Users hit hard failures when the client and server codec numbers differ ([issue #3141](https://github.com/wez/wezterm/issues/3141), search summary). **Lesson: a server-rendered protocol becomes a version-lock problem.** |
| **kitty** | The kitty process | kitty | **No detach.** "Sessions" are layout files. 0.43+ adds one-key switching and saving the current layout as a session ([sessions](https://sw.kovidgoyal.net/kitty/sessions/), [changelog](https://sw.kovidgoyal.net/kitty/changelog/), search summary) | Same tier as fuxx M7. |
| **cmux** | The app process for ordinary local terminals ([issue #10651](https://github.com/manaflow-ai/cmux/issues/10651)): "Ordinary local terminal PTYs and processes are still owned by the cmux app process". | — | PR #10657 adds an **opt-in `cmux local-tmux`**: a dedicated tmux server per user under `~/.cmux/local-tmux`, and a registry mapping stable session UUIDs to tmux identities. It "cannot survive logout, restart, shutdown, or power loss". | [PR #10657](https://github.com/manaflow-ai/cmux/pull/10657) (fetched; the page itself is inconsistent about merged versus pending). Remote: `cmux ssh` uploads a versioned `cmuxd-remote` (JSON-RPC over stdio) that manages remote sessions and resizing ([SSH docs](https://cmux.com/docs/ssh), verified). **cmux chose to reuse tmux locally, not to write a daemon.** |
| **zmx** | **One daemon per session** (unix socket per session, `poll(2)` loops) | The daemon feeds the same bytes to `libghostty-vt`, which "holds terminal state and scrollback" and is not on the hot path | `ghostty-vt` "sends terminal snapshot to client stdout" to re-hydrate the client | [zmx README](https://github.com/thinety/zmx) (fetched). Known issue: upgrading across IPC changes "will kill all your sessions". **This is the recommended design minus the GUI, in miniature.** |
| **dtach / abduco** | A tiny holder process | **Nobody.** "dtach does not keep track of the contents of the screen", so it relies on programs redrawing (^L or SIGWINCH) | A blank screen plus a redraw signal | [dtach](https://dtach.sourceforge.net/) (verified). abduco recommends running apps under dvtm if redraws are incomplete ([abduco](https://www.brain-dump.org/projects/abduco/), verified). Fine for vim, poor for shells and for agent transcripts. |
| **Claude Code `--bg` / agent view** | A supervisor `bg-pty-host` daemon | The daemon | `claude agents` attaches | Relevant mainly as **a warning**: its detached host, reparented to launchd, got `EPERM` on `~/Documents` despite Full Disk Access, because TCC "Failed to create Attribution Chain" ([issue digest #59065](https://claudeissues.com/issue/59065-bug-bg-pty-host-children-blocked-by-macos-tcc-on-documents-despite-fda-grant-fai), third-party mirror, unverified; marked fixed 2026-05-16 without saying how). |

**Takeaways:**
1. Everyone that survives a GUI quit has a separate process holding the PTY master. Nothing else works, because the
   kernel hangs up the PTY when the last master fd closes.
2. The fully server-rendered design (WezTerm, zellij) moves the whole rendering contract onto a wire protocol and
   ends up version-locked.
3. The "server keeps a state copy, client keeps its own emulator, raw bytes on the wire, snapshot on attach" design
   (tmux `-CC`/iTerm2, zmx) keeps the client fast and the protocol small.
4. **None of them survive logout or reboot.** After a reboot, fuxx still needs M7's cold restore.

### 2.2 Options

- **1. Server-rendered.** The daemon owns the PTY and the `Term`. The GUI receives screen state or diffs and sends
  input.
- **2. Raw-bytes only.** The daemon owns only the PTY and streams bytes. The GUI owns the `Term` and replays a byte
  log or snapshot on reattach.
- **2b. Hybrid (recommended).** The daemon owns the PTY **and** an authoritative headless `Term`. The GUI owns a
  replica `Term`. Raw bytes go on the wire, and a VT-encoded snapshot is sent on attach.
- **3. SCM_RIGHTS fd handoff.** The GUI passes the PTY master fds to another process on quit and gets them back on
  relaunch.

| Criterion | 1 Server-rendered | 2 Raw bytes, GUI-only Term | **2b Hybrid** | 3 SCM_RIGHTS handoff |
|---|---|---|---|---|
| **Latency / CPU** | Keystroke: +1 socket hop. Output: the daemon must compute damage and serialize cells every frame. A full-screen TUI redraw of 200×60 is about 12k cells × ~16 B ≈ 200 KB per frame (estimate), so up to ~12 MB/s at 60 fps under a churning TUI. Scrolling needs `GetLines`-style fetches, which WezTerm needs predictive echo to hide. | +1 hop each way. Single parse. | +1 hop each way. **Two parses** (daemon and GUI); alacritty's parser is cheap next to GPU paint (estimate, measure in the spike). Scrolling, selection and hover are local and free. | No overhead while running. |
| **Scrollback on reattach** | Lazy fetch from the daemon | Only as good as a bounded byte log. Replay starts mid-sequence and loses earlier modes and colours. Agent TUIs redraw constantly, so the log is mostly churn. | **Exact up to the daemon's 10k lines**, as a snapshot bounded by grid size, not by output volume | Lost unless the old process also serializes its `Term` |
| **Status detection** | In the daemon (has the `Term`) | Stuck in the GUI. Nothing tracks status while the window is closed, and rings reset to Idle on reopen. | **In the daemon**: `detector.rs` and `status.rs` move unchanged. Rings come back correct, waits keep counting. | In the GUI, gone while quit |
| **Query replies while no GUI** (DA, CPR, OSC 10/11, `CSI 14 t`) | Daemon answers | **Nobody answers**, so programs that query at startup hang (the M1 lesson in `terminal_view.rs`) | **Daemon answers** (it has the `Term`; the GUI pushes the theme and cell size) | Nobody |
| **Crash safety** | GUI crash: safe. Daemon crash: all lost. | Same | Same. Daemon is GPUI-free and small; a per-session reader-thread panic freezes one session, not the process (alacritty's `FairMutex` is parking_lot, no poisoning). | **A GUI crash still kills everything**, because no one is holding the fds. That fails the main goal. |
| **Code moved** | Large: selection, links, `read`, scroll, mouse modes and the paint input (`GridSnapshot`) all go behind RPC | Small, but the daemon must grow a `Term` anyway for replies, status and `fuxx read` while closed, which turns it into 2b | **Medium**: `TerminalCore` splits into a backend (local PTY or daemon session) plus the existing replica. The view drops its query replies and tracker. Render/selection/links/mouse/grid are untouched. | Small code, but you still need a holder process, which is a daemon |
| **Upgrade (new GUI, old daemon)** | Hard: render protocol version lock (WezTerm codec) | Easy | **Easy-ish**: the payload is VT bytes, the terminal lingua franca, which any `Term` version can parse. The control messages are small JSON with additive fields. | n/a |
| **Security** | Same-user socket as M9 | Same | Same | fd passing over a same-user socket; fine |
| **M9 scripting** | Daemon could serve it | GUI only | Either; see §2.3.9 | GUI only |
| **M7 restore** | Kept for reboot | Kept | Kept for reboot, extended to reattach | Kept |

**Verdict:**
- **Option 3 is not a persistence design.** It fails crash safety and still needs a holder. Its one real use is a
  later **daemon hot-upgrade**: an old daemon hands its PTY fds and serialized `Term`s to a new daemon, or execs
  itself in place with the fds left open.
- **Option 2 collapses into 2b** as soon as you need query replies, status or `fuxx read` while the window is closed.
- **Option 1 is what tmux and zellij need** because their client *is* a dumb terminal. fuxx's client is a full GPU
  emulator, so shipping cells would throw that away and version-lock the protocol.

### 2.3 Recommended design (2b, hybrid)

#### 2.3.1 Shape

```
            fuxx.app (GUI, GPUI)                                   fuxx daemon (no GPUI, same binary)
 ┌──────────────────────────────────────────┐            ┌────────────────────────────────────────────────┐
 │ Workspace ── sessions.json (layout only) │            │ accept thread (0600 sock, getpeereid)          │
 │ TerminalView ── paint/selection/links    │            │ per session:                                   │
 │   └ TerminalCore                         │  fuxxd.sock │   Pty (master fd, child)                       │
 │       replica Term + Processor  ◄────────┼── Output ──┤   reader thread: detector → Term.advance       │
 │       backend = Daemon(session, conn) ───┼── Input ──►│        → fan-out Output{seq} to subscribers    │
 │                                  ────────┼── Resize ─►│   writer thread (input queue)                  │
 │ ignores replica PtyWrite/Color/14t       │◄─ Status ──┤   authoritative Term (replies, read, snapshot) │
 │ M9 fuxx.sock server (unchanged in M10a)  │◄ Snapshot ─┤   StatusTracker (ticked by one 50 ms thread)   │
 └──────────────────────────────────────────┘            │ metadata: id s<N>, name, project, preset,      │
                                                          │   title, note, progress, theme, cell px        │
                                                          └────────────────────────────────────────────────┘
```

#### 2.3.2 Module sketch (new and changed files)

| File | Kind | What |
|---|---|---|
| `src/vt_snapshot.rs` | **new, pure** | `encode(&Term) -> Vec<u8>`: a VT byte stream that rebuilds the `Term` on a fresh one. Contents: reset; scrollback + screen rows with SGR runs; wide chars and zero-width; **soft wraps (`WRAPLINE`) emitted as overflow, not `\r\n`, so reflow still works**; OSC 8 hyperlinks; if the alt screen is active, primary contents first, then `CSI ?1049h` and alt contents; scroll region; cursor position, shape and visibility; modes (DECCKM, bracketed paste, mouse modes and encoding, focus reporting, origin, autowrap, kitty keyboard flags `CSI = n u`, DEC 2026 off); title (OSC 2); then the cursor. **Tested by round-trip**: feed recorded byte streams (Claude Code, vim, htop, `ls --color`) into Term A, encode, feed into fresh Term B, assert cells, wraps, cursor and modes are equal. This is TDD-friendly and the main correctness risk (§2.4). |
| `src/mux_proto.rs` | **new, pure** | Framing plus messages; encode/decode; handshake and version range (§2.3.3). |
| `src/daemon.rs` | **new** | `fuxx daemon` entry point: socket bind (reusing the `api_server` patterns: 0700 dir, 0600 socket, `getpeereid`, connection cap, bounded queues, stale-socket replacement); session table; tick thread; idle-exit; log file. |
| `src/daemon_session.rs` | **new** | One session: `Pty`, authoritative `Term` + `Processor` (today's `Shared`), `OscDetector`, `StatusTracker`, seq counter, subscribers, query replies (moved from `terminal_view.rs::on_term_event`), and `flush_stale_sync`. Most of it is today's `TerminalCore::spawn` body. |
| `src/daemon_client.rs` | **new** | GUI side: connect or spawn the daemon (§2.3.5); one reader thread that demuxes frames per session into each `TerminalCore`'s channel; a writer with a bounded queue. |
| `src/terminal.rs` | **changed** | `TerminalCore` gets `enum Backend { Local(Pty, input), Daemon{session, client} }`. `write`/`input`/`resize` go to the backend. In daemon mode the replica is fed from `Output` frames, applies `Resized` when it reaches that frame, and starts from a `Snapshot`. Local mode stays for `persistent-sessions = off` and the tests. |
| `src/terminal_view.rs` | **changed (~60 lines)** | In daemon mode: drop the replica's `PtyWrite`/`ColorRequest`/`TextAreaSizeRequest` (the daemon answers); take `StatusChanged`, `Title` and `Exited` from daemon frames instead of the local tracker. |
| `src/workspace.rs` | **changed (~200 lines)** | Ids come from the daemon (`s<N>` stays stable across GUI restarts). `open_session` → `client.create(...)`. Startup: `List` → adopt live sessions, place them using `sessions.json` (matched by daemon id), cold-restore the rest. Close = kill; quit = detach (§2.3.6). |
| `src/restore.rs` | **changed (small)** | `SavedSession` gains `#[serde(default)] daemon_id: Option<u64>`. Additive, so `VERSION` stays 1. |
| `src/workspace_api.rs` | **changed (small)** | Ids and status come from the daemon-backed sessions. Otherwise unchanged in M10a. |
| `src/cli.rs`, `src/main.rs` | **changed** | `fuxx daemon [run]` (foreground, for launchd or debugging), `fuxx daemon status`, `fuxx daemon stop [--force]`. |
| `src/notifier.rs` | maybe | Only if the owner wants notifications while the window is closed (Q2). |

**Build hygiene for M10b Option B later:** keep `daemon*.rs`, `mux_proto.rs`, `vt_snapshot.rs`, `pty.rs`,
`status.rs` and `detector.rs` free of `gpui` imports, so a later `--no-default-features` Linux build of the daemon is
possible. Today `main.rs` declares every module in one binary crate; a `lib.rs` split or a `gui` cargo feature is
the cheap prep step.

#### 2.3.3 Wire protocol

- A separate socket, `~/Library/Application Support/fuxx/fuxxd.sock`. Same folder and permissions as M9; not
  `fuxx.sock`, so M9 is undisturbed.
- **Binary framing.** M9's JSON lines would force base64 on every output byte.
  - Frame: `u32 BE length | u8 kind | payload`.
  - Hot-path frames (`Input`, `Output`) carry a fixed binary header (`session u64`, `seq u64`) followed by raw
    bytes.
  - Everything else is a JSON payload (serde_json is already a dependency) with **additive, ignored-if-unknown
    fields**.

| Direction | Message | Purpose |
|---|---|---|
| C→D | `Hello{proto_min, proto_max, app_version, role:"ui"\|"script"}` | Handshake. The daemon replies `HelloOk{proto, daemon_version, pid}` or `Incompatible{proto_min, proto_max}`. |
| C→D | `List` → `Sessions[{id, name, project, preset, title, status, note, progress, dims, alive, created_at}]` | Startup adoption; enough to draw the sidebar before attaching anything |
| C→D | `Create{cwd, startup, env, dims, cell_px, name, project, preset}` → `Created{id}` | `env` is **the GUI's environment** (see the stale-env note in §2.3.7) plus the `FUXX_*` scripting variables |
| C→D | `Attach{id}` / `Detach{id}` | Attach replies `Snapshot{id, seq, dims, vt_bytes}`, then streams `Output{seq>snapshot.seq}` |
| C→D | `Input{id, bytes}` | Keystrokes, paste, scripted send. The daemon calls `tracker.on_input`. |
| C→D | `Resize{id, cols, rows, cell_w, cell_h}` | The daemon resizes `Term` + PTY, then emits `Resized{id, seq, cols, rows}` **into the output sequence**, so the replica reflows at the same byte position as the daemon. |
| C→D | `SetTheme{palette}` | So OSC 4/10/11/12 replies match what's on screen. The daemon keeps the last one for when no GUI is attached. |
| C→D | `SetMeta{id, name?, project?, preset?, note?, progress?}` | Metadata lives in the daemon so it survives GUI restarts and `List` can show it |
| C→D | `Close{id}` | Kill the child (today's `Drop for Pty`) |
| C→D | `ReadText{id, scope}` | For headless scripting later; cheap to add now |
| D→C | `Output{id, seq, notifications, bytes}` | Raw PTY bytes, as read |
| D→C | `Status{id, status}`, `Title{id, Option<String>}`, `Bell{id}`, `Exited{id}` | Events the GUI used to derive itself |
| D→C | `Resync{id}` | That subscriber's queue overflowed. The daemon dropped its backlog; the client re-`Attach`es and gets a fresh snapshot. This is tmux's `pause-after` + `capture-pane` idea: memory stays bounded and a flood skips frames instead of lagging. |

**Ordering invariant:** the reader thread does three things under the session lock: `Term.advance`, `seq += 1`, and
enqueue to subscribers. `Attach` encodes the snapshot and registers the subscriber under the same lock. So the
snapshot at seq N and the stream from N+1 never overlap or leave a gap. This can be unit-tested.

**Replica divergence:** the replica sees the same bytes and the same resize points as the daemon, so with the same
alacritty version it ends up in the same state. With different versions (new GUI, old daemon) it can drift
cosmetically. That's harmless, because the daemon's `Term` is the authority for replies, status and `read`, and any
`Attach` re-syncs.

#### 2.3.4 Status, notifications, query replies

- `OscDetector` stays in the reader thread, now in the daemon.
- `StatusTracker` moves into `daemon_session.rs`:
  - `on_output` from the reader;
  - `on_bell` from the `Term` listener;
  - `on_input` from `Input` frames (every client input is user or script input; replies never come through the
    client now);
  - `tick` from one 50 ms daemon thread for all sessions, which also runs `flush_stale_sync`.
- The daemon emits `Status` frames. The GUI's rings, `wait` and events consume them exactly as they consume
  `TerminalEvent::StatusChanged` today.
- **Benefit:** status keeps evolving while the window is closed, and rings are correct on reopen. Today they would
  reset to Idle.
- Notifications stay in the GUI (focus-aware) unless the owner wants them while the window is closed (Q2). In that
  case the daemon calls `notifier.rs` (osascript) when no UI client is attached.

#### 2.3.5 Daemon lifecycle

- **Spawned on demand by the GUI.** Not launchd.
  - At startup the GUI connects to `fuxxd.sock`. If there's no live daemon (connect refused, or the socket is
    stale), it spawns `current_exe() daemon` with `posix_spawn` + `POSIX_SPAWN_SETSID`, stdio pointed at
    `~/Library/Logs/fuxx/daemon.log`, and fds closed. Then it retries the connect for up to about 2 s.
  - This is the tmux, zmx and WezTerm (`serve_command`, `--daemonize`) model.
- **Why not launchd:**
  - A LaunchAgent also dies at logout, so launchd buys **no extra survival**.
  - Restarting a crashed daemon via `KeepAlive` recovers no sessions, because their PTYs are gone.
  - `SMAppService` wants bundle-embedded plists and in practice a signed app; it can also sit in "requires
    approval" ([Apple: updating helpers](https://developer.apple.com/documentation/servicemanagement/updating-helper-executables-from-earlier-versions-of-macos),
    [forum](https://developer.apple.com/forums/thread/779379), search summaries).
  - Revisit at M12 (signing), only if the TCC spike says the daemon needs its own identity.
- **Stays alive** while it has sessions, with or without a GUI. **With zero sessions and zero clients, it exits
  after a grace period** (e.g. 60 s), so it never lingers invisibly. The GUI respawns it when needed.
- **`fuxx daemon stop`** refuses while sessions are alive and names them. With `--force` it hangs up every session
  (SIGHUP to each process group, like closing a terminal) and exits. `fuxx daemon status` prints pid, version,
  protocol, uptime and the session count.
- **Logout and reboot:** the daemon dies with the user session, as tmux and cmux's local-tmux do. The next launch
  finds no daemon, and M7 cold restore runs.

#### 2.3.6 Quit, close, and M7 restore

- Closing a session (cmd-w or the close button) still **ends** it.
- Quitting the app or closing the window **detaches**: sessions keep running. The menu needs both:
  - "Quit fuxx" ("sessions keep running" when N > 0);
  - "Quit fuxx and End All Sessions" (e.g. opt-cmd-q).
  - Whether persistence is the default or opt-in is **Q1**. cmux made its version opt-in.
- **Keep M7's restore.** It becomes the cold path after a reboot or daemon death, and the warm path's layout source:
  - On launch: `List` the live sessions.
  - Place each one using `sessions.json` (layouts, groups, collapsed, active), matched by `daemon_id`.
  - Sessions in `sessions.json` that aren't live → `RestoreMode` as today (Agents re-runs the preset).
  - Live sessions not in `sessions.json` (created by a script while the window was closed) → appended as their own
    layouts.
  - `session-restore = off` keeps meaning "don't recreate dead sessions". Live sessions are always shown; hiding
    them would leak running processes.
- **Lazy attach:** draw the sidebar from `List` straight away, `Attach` (snapshot) only the sessions on screen, and
  attach the rest on first view or in the background. This keeps launch fast with 20 sessions × multi-MB scrollback,
  and halves GUI memory for sessions never looked at.

#### 2.3.7 Correctness traps the design must handle

1. **Session ids must become daemon-owned and stable.** A shell that outlives a GUI restart keeps
   `FUXX_SESSION=s3` in its environment forever. With today's per-launch ids, `s3` could mean a different session
   after relaunch, and `fuxx send` from that shell would hit the wrong session. (This also removes M9's "ids are
   fresh each launch" limitation while the daemon lives.)
2. **Stale environment.** A long-lived daemon's own env is frozen at spawn (tmux's `update-environment` problem).
   Pass the GUI's env with each `Create`. Don't use the daemon's own.
3. **Theme replies with no GUI:** keep the last `SetTheme`; until the first one, use the default theme.
4. **`CSI 14 t` with no GUI:** keep the last cell px.
5. **Resize authority:** one UI client at a time; a new UI attach takes over, like `tmux attach -d`. Script clients
   never resize.
6. **Memory:** scrollback exists twice, in the daemon and the replica (up to 10k lines each). Lazy attach (above)
   limits the GUI side. Acceptable.
7. **Old binary on disk:** after an app upgrade, the daemon keeps running the old code, which is fine. `current_exe`
   for the PATH addition must be the GUI's, passed in `Create`, not the daemon's.

#### 2.3.8 Security

- Same model as M9 and tmux: socket 0600 in a 0700 folder, plus a `getpeereid` uid check on every connection.
  Nothing listens on the network.
- **New exposure:** a long-lived process holds every session's screen and accepts input for them, *also while no
  window is open*.
  - Any same-user process could already do this via M9 while fuxx was open, and via tmux's socket in general.
  - Document it.
  - Don't add a token: same-user code can read the token anyway.
- The daemon must never accept control messages **in-band from PTY output**. iTerm2's SSH "conductor" did, and
  `cat readme.txt` became code execution ([CVE-2026-41253 write-up](https://lilting.ch/en/articles/iterm2-cve-2026-41253-ssh-conductor-pty-confusion),
  unverified secondary source). In the recommended protocol, control flows only over the socket.

#### 2.3.9 What happens to M9 scripting

- **For M10a: keep the M9 server in the GUI.** M9's handlers resolve targets against GUI state (layouts, focus,
  sidebar order `#2`, splits), and `new --split`, `focus` and `equalize` are inherently GUI operations. With the
  daemon as the backend, `send`/`read`/`status`/`wait` just work, because the GUI's sessions are daemon-backed.
- **Follow-up (optional, M10a.2, size M): make the daemon the scripting endpoint**, so `fuxx ls/send/read/status/
  wait/question/answer` and `fuxx mcp` work **with the window closed**. The GUI would register as the handler for
  GUI-only methods (`focus`, `split`, `equalize`, `#N` targets); without a GUI, those return a clear `no_window`
  error.
  - This is the more powerful end state (agents driving agents while the window is closed).
  - It moves `workspace_api.rs` logic and the `WaitTable` into the daemon, which is the reason to do it second.
  - **Q3 decides whether it's in M10a.**

#### 2.3.10 Upgrade path

- `Hello` negotiates a protocol range from day one. Keep changes additive: JSON fields ignored when unknown, new
  frame kinds ignored by old peers (WezTerm's `Pdu::Invalid{ident}` trick).
- New GUI + old daemon with a compatible protocol: fine. The replica may differ cosmetically; any `Attach` re-syncs.
- An incompatible protocol, which should be rare: the GUI shows "N sessions are running in an older fuxx background
  service. [Restart it (ends N sessions)] [Keep them running]". "Keep" leaves the old daemon serving `fuxx` in those
  shells until they end; the new GUI opens a second daemon on a versioned socket name (`fuxxd-v2.sock`).
- **Later (optional):** hot-upgrade by fd handoff. The old daemon sends its PTY master fds over `SCM_RIGHTS`, plus
  each session's `vt_snapshot`, to the new daemon. Or it `execv`s the new binary with the fds left open. This is the
  one good use of Option 3; it's what zmx's "upgrades kill sessions" known issue lacks. Not needed for M10a.

#### 2.3.11 TCC and launch context (must be a spike before task 2)

- Children inherit their parent's TCC "responsible process" ([Qt blog](https://qt.io/blog/the-curious-case-of-the-responsible-process),
  [Apple forum](https://developer.apple.com/forums/thread/805245), partly unverified).
- Today a shell's file access to `~/Documents` is attributed to **fuxx.app**.
- A daemon spawned by fuxx.app and **outliving it** may hit Claude Code's "Failed to create Attribution Chain" →
  `EPERM`. That's unverified for fuxx, but it is exactly this shape.
- The spike:
  1. Spawn the daemon from the `.app` (Finder launch).
  2. Quit the GUI.
  3. In a surviving session, `ls ~/Documents ~/Desktop` and `touch` a file there.
  4. Repeat with `POSIX_SPAWN_SETDISCLAIM` (via `responsibility_spawnattrs_setdisclaim`, a private API) so the
     daemon is responsible for itself.
- If the disclaimed daemon prompts as "fuxx" and works, use that. Caveat: unsigned/ad-hoc builds may re-prompt after
  every update until M12 signing.
- If neither works, persistence must say "folders protected by macOS privacy (Documents, Desktop, Downloads) may be
  unreadable after fuxx quits", which is a real product limit.

### 2.4 Key risks (M10a)

1. **TCC attribution after the GUI quits** (§2.3.11). This could break agents working in `~/Documents` projects.
   Spike first.
2. **Snapshot fidelity.** Anything `vt_snapshot` misses (tab stops, G0–G3 charsets, saved cursor, keyboard-mode
   *stack* depth, a pending synchronized update, images if ever added) shows up as a slightly wrong screen after
   reattach until the program redraws. Agent TUIs redraw often, so this mostly self-heals. The round-trip tests
   bound it.
3. **Daemon crash = all sessions lost**, the same blast radius as today's GUI crash, but now in a process with no
   GPUI or Metal and far less code. Mitigations: thread-per-session isolation, no `panic = "abort"`, and a crash log.
4. **Hidden-process surprise.** Users may not realise agents are still running (and spending tokens) after
   quitting. Mitigations: the quit menu wording, a Dock/menu count, `fuxx daemon status`, and idle-exit at zero
   sessions.
5. **GUI architecture churn.** `TerminalCore`'s two backends double the test matrix. Keeping Local mode lets every
   existing test keep running unchanged.

### 2.5 Task breakdown (M10a)

| # | Task | Size | Verifiable how |
|---|---|---|---|
| 0 | **Spike** (throwaway): detached daemon from the `.app`; survives cmd-q and `kill -9` of the GUI; TCC check (§2.3.11); keystroke-echo round-trip through a unix socket versus direct | 1–2 days | Manual + timing numbers recorded in the milestone note |
| 1 | `vt_snapshot.rs` + round-trip tests on recorded streams (Claude Code, vim, htop, colored ls, alt screen, wide chars, OSC 8, wrapped lines + resize) | M | Unit tests |
| 2 | `mux_proto.rs` framing + messages + handshake | S | Unit tests |
| 3 | `daemon_session.rs`: move today's spawn, reader, writer, replies, detector, tracker; seq + subscribers + resync | M | Unit tests with `sh -c` children, as in `terminal.rs` tests |
| 4 | `daemon.rs`: socket, sessions table, tick thread, idle exit, `daemon run/status/stop` | S–M | Unit + CLI tests |
| 5 | `daemon_client.rs` + `TerminalCore` Daemon backend + replica feeding + resize-at-seq | M | Integration tests: daemon in-process on a temp socket |
| 6 | `TerminalView`/`Workspace`: status/title/exit from the daemon; stable ids; create/close/detach; quit menu items | M | Unit + 👀 manual |
| 7 | Startup adoption + `sessions.json` `daemon_id` + lazy attach + cold-restore fallback | S–M | Unit (pure placement logic) + 👀 |
| 8 | Setting `persistent-sessions = on\|off` + a Preferences row; docs, README, STRUCTURE | S | 👀 |
| 9 | (Optional, Q2) daemon-side notifications while no GUI | S | 👀 |
| 10 | (Optional, Q3 → or M10a.2) daemon serves M9 headless | M | Existing M9 tests re-pointed |

**Total: L (≈ one M9) without 9–10; L+ with them.** Success check: start `claude` in two sessions, one split; cmd-q;
wait while one asks a question; reopen; both sessions are back with full scrollback, the layout is intact, the
waiting one shows the waiting ring, and `fuxx answer` works. Separately, `kill -9` the GUI and get the same result.

---

## 3. M10b: SSH workspaces

### 3.1 What others offer

| Tool | Model | Notes |
|---|---|---|
| **cmux** | `cmux ssh user@host`: reads `~/.ssh/config`; uploads a versioned `cmuxd-remote` (SHA-256-checked against a manifest in the app) that speaks JSON-RPC over stdio and manages remote sessions; reconnect backoff 3/6/12…60 s; smallest-screen-wins resize; `ServerAliveInterval=20`; remote `cmux notify` reaches the local sidebar via a **reverse TCP tunnel with HMAC-SHA256**; the browser proxies via SOCKS5/CONNECT through the daemon; a trust prompt; `host-key-policy`; optional mosh + tmux | [cmux SSH docs](https://cmux.com/docs/ssh) (verified). The feature-complete end of the spectrum: effectively Option B plus extras. |
| **WezTerm** | `SSH:host` (plain, local ssh client in a pane) **and** `SSHMUX:host` (spawns `wezterm-mux-server` remotely; sessions persist across drops). Both are generated automatically from `~/.ssh/config`. Remote needs a compatible wezterm version. | [Multiplexing docs](https://wezterm.org/multiplexing.html) (verified). **Offers both A and B, and A is the default.** |
| **iTerm2** | Plain ssh, plus "SSH integration" (`it2ssh`) that bootstraps a "conductor" script over the session's own terminal I/O | In-band protocol → CVE-2026-41253 ([write-up](https://lilting.ch/en/articles/iterm2-cve-2026-41253-ssh-conductor-pty-confusion), unverified). **Anti-pattern for fuxx.** |
| **Warp** | "Warpify" subshells (detects `ssh`, injects shell integration; needs bash/zsh/fish remote) and an SSH extension with a remote file tree | [Warp subshells](https://docs.warp.dev/terminal/warpify/subshells) (search summary) |
| **VS Code Remote-SSH** | Installs a VS Code Server on the remote over SSH; everything (terminals, extensions) runs there; the client is UI only | [VS Code Remote-SSH](https://code.visualstudio.com/docs/remote/ssh) (from memory, unverified today). The Option B archetype, with a big install-and-version-management burden. |
| **Zed** (the GPUI source fuxx pins) | Remote projects run a `remote_server`, but **terminals are local `ssh -t` processes sharing a ControlMaster socket** (`crates/remote/src/transport/ssh.rs`: `ControlMaster=yes`, `ControlPath=…`, `-t`; reuses an existing master via `ssh -G`) | Verified in the pinned source. **Precedent for Option A.** |

### 3.2 Options

- **Option A: a local `ssh` session** (recommended now).
  - An "SSH session" is an ordinary fuxx session whose command is `ssh [-t] host [remote command]`.
  - **Status detection works unchanged:** `OscDetector` and `StatusTracker` read the byte stream, and ssh is a
    transparent byte pipe, so OSC 9 / 99 / 777 and BEL from a remote agent arrive exactly as from a local one. fuxx
    already relies on BEL and screen heuristics more than OSC.
  - `TERM=xterm-256color` (`pty.rs`) is in every remote terminfo, so there's no Ghostty-style terminfo-copying
    problem.
  - **Remote persistence (optional):** remote command `tmux new -A -s fuxx-<name>`. Caveats:
    - inside tmux, OSC 9/777 need `set -g allow-passthrough on` (tmux ≥ 3.2) **and** the app must wrap them in DCS
      passthrough ([pi-notify](https://github.com/ferologics/pi-notify), [tnotify](https://pkg.go.dev/github.com/soloterm/tnotify@v0.1.1),
      unverified);
    - BEL forwarding depends on tmux's `bell-action`/`monitor-bell` (unverified);
    - the screen heuristics (`WAITING_PATTERNS`) still work, because tmux redraws the pane into fuxx's screen.
  - **With M10a in place**, the local ssh client lives in the daemon, so an SSH session survives a fuxx quit or crash
    with no remote tmux at all. Only network drops and laptop sleep end it.
- **Option B: a remote fuxx daemon over SSH** (deferred).
  - `ssh host fuxx daemon --stdio` on an **exec channel**, not in-band in a PTY, speaking the M10a protocol.
  - Gives persistent remote sessions across network drops, remote-side status, and **remote `fuxx`/MCP that talks to
    the remote daemon locally**, with no reverse tunnel.
  - Costs:
    - a GPUI-free Linux daemon build (x86_64 + aarch64, likely musl);
    - upload and version pinning with a checksum (cmux) or a version-tagged install path (VS Code);
    - reconnect with backoff;
    - snapshot transfer over WAN (send the screen first and history lazily, or cap it);
    - protocol version skew across machines.
  - **XL.** Its value is real (this is the actual cmux remote daemon), but it should come after M10a has proved
    the protocol locally.

### 3.3 Recommended design (Option A)

- **Host picker.**
  - "New SSH session…" (and `fuxx new --ssh HOST`) lists `Host` entries from `~/.ssh/config`, following `Include`
    and skipping wildcard patterns.
  - Resolve each with `ssh -G host` (prints the effective config) for display, never re-implementing ssh config
    semantics.
  - Plus recent hosts. Free text is allowed but is passed as **one argv element**, never through a shell.
  - Reject leading `-` to avoid option injection (`ssh -oProxyCommand=…`).
- **Grouping.**
  - `project` becomes a host key such as `ssh://host`, so sessions group by host with the rolled-up ring (M7
    groups).
  - An optional remote folder: `ssh -t host 'cd <dir> && exec $SHELL -l'`, with the directory shell-quoted
    remotely. Better: offer the folder only via the tmux option or a preset.
- **Connection reuse.** Pass `-o ControlMaster=auto -o ControlPath=~/Library/Application Support/fuxx/ssh/%C
  -o ControlPersist=10m`, unless the user's config sets them (check with `ssh -G`). Then a second pane to the same
  host opens instantly and authenticates once, as in Zed.
- **Keepalive.** `ServerAliveInterval=20`, `ServerAliveCountMax=2` unless configured, following cmux. A dead link is
  then noticed in about 60 s instead of hanging.
- **Reconnect.**
  - When ssh exits with code 255 (a connection error; the remote shell's own exit is shown as is), the pane keeps
    its screen and shows a one-line banner "Disconnected from host — press Enter to reconnect", and the ring shows a
    new or "done" state.
  - Auto-reconnect with backoff only when the session uses remote tmux, because reattach is then meaningful. A
    plain shell would come back as a *new* shell, which is surprising if done silently.
- **Restore (M7).** `SavedSession` gains `ssh: Option<{host, tmux: bool}>`. Cold restore re-runs ssh: with tmux this
  is a true reattach; without it, a fresh remote shell.
- **Remote scripting (not in A).**
  - `FUXX_SOCKET` is meaningless on the remote. Forwarding the local socket (`ssh -R remote.sock:local.sock`) would
    let anything on the remote host (root, or the same remote user) **type into your local sessions, which is local
    code execution**.
  - If ever wanted: opt-in per host, restricted to `notify/note/progress/status`, and authenticated (cmux uses HMAC
    over a reverse tunnel). Otherwise wait for Option B.
  - Also: don't invent in-band OSC control codes beyond the standard notification/progress OSCs (the iTerm2 CVE
    lesson).
- **Environment.** `FUXX_SESSION` doesn't cross ssh: OpenSSH sends only what `SendEnv`/`SetEnv` allow, and servers
  usually `AcceptEnv LANG LC_*` only (common distro default, unverified per host). Don't depend on it.

**Size: A ≈ S–M** (picker + `ssh -G` + command building with tests + reconnect banner + restore field + grouping
key; about 600–1,000 lines). Success check: open two panes to a host from the picker (the second opens instantly);
run `claude` remotely; the ring goes to waiting when it asks; drop Wi-Fi → the reconnect banner appears; Enter
reconnects; quit fuxx (with M10a) → reopen → the ssh session is still connected.

**Risks:**
- interactive auth prompts (passwords, 2FA, host-key confirmation) happen *in the pane*, which is fine and arguably
  correct, but means there's no headless connect;
- ControlMaster sharing means one dead master stalls every pane to that host until `ControlPersist` times out
  (mitigate with `ssh -O check` before reuse);
- remote tmux passthrough configuration is the user's to set.

---

## 4. M10c: in-app browser

### 4.1 Can a WKWebView be embedded at GPUI rev 1d2a4b3? (verified in source)

- **Window handle:**
  - `impl HasWindowHandle for gpui::Window` (`crates/gpui/src/window.rs:6138`) delegates to the platform window.
  - On macOS (`crates/gpui_macos/src/window.rs:1916`) it returns `RawWindowHandle::AppKit` whose `ns_view` is
    **GPUI's own Metal-backed view**.
  - `raw-window-handle 0.6.2` is in `Cargo.lock`.
- **GPUI's view setup:**
  - GPUI creates its `native_view` (a custom NSView class with `makeBackingLayer` returning the renderer's Metal
    layer, `setWantsLayer(YES)`, and **no `isFlipped` override**, so the origin is bottom-left).
  - It adds that view as a subview of the window's content view and calls `makeFirstResponder(native_view)` **once,
    at window creation** (`window.rs:950-985`).
  - Its view overrides `performKeyEquivalent:`, `keyDown:` and the mouse events (`window.rs:141-288`).
- **Zed embeds no native views.** `addSubview` appears only in `gpui_macos/src/window.rs`, for its own view, and
  nothing in the tree references `WKWebView` or `wry`.
- The only "foreign pixels" path is `Window::paint_surface(bounds, CVPixelBuffer)` (`window.rs:4217`; the
  `surface()` element is `elements/surface.rs`), meant for video frames. Feeding it `WKWebView` snapshots would not
  be interactive (snapshots are async and slow), so it's not a browser path.
- **Bindings already in `Cargo.lock` via GPUI:** `objc2 0.6.4` (+0.5.2), `objc2-app-kit 0.3.2`,
  `objc2-foundation 0.3.2`, `objc2-quartz-core`, `block2 0.6.2`, `cocoa 0.26`, `objc 0.2.7`. **Not present:**
  `objc2-web-kit` or `wry`, which would be new dependencies (versions to be checked on Context7/crates.io at the
  time).
- **wry** (Context7 `/tauri-apps/wry`, verified today): `WebViewBuilder::build_as_child<W: HasWindowHandle>(&W)` →
  on macOS an **"NSView subview"**; `with_bounds`/`set_bounds(Rect)` (effective only for child webviews);
  `set_visible`; `focus()` / `focus_parent()`.
- So `wry::WebViewBuilder::new().with_url(..).build_as_child(window)` would add a `WKWebView` **as a subview of
  GPUI's Metal view**.

**Feasible: yes. Clean: no.**

### 4.2 Options and their pitfalls

| Option | What | Pitfalls |
|---|---|---|
| **(a) Child NSView over a pane** (via wry `build_as_child`, or raw objc2 `addSubview:` as a sibling above GPUI's view in the content view) | A real browser in a split | **Z-order:** native views always sit above GPUI's Metal content, so anything GPUI draws over that rectangle disappears beneath it: drag previews, the rename box, menus/popovers, M11 overlays, divider-drag feedback. You must hide or move the webview whenever an overlay opens. **Clipping:** it ignores GPUI content masks and rounded corners. **Bounds sync:** set the frame from a `canvas` prepaint each frame when changed; convert the bottom-left (unflipped) origin and scale. During live resize and divider drags the native frame lags the Metal frame by a frame (visible jitter). **Focus:** GPUI's `FocusHandle` world doesn't know the webview took first responder. fuxx must call `makeFirstResponder(gpui_view)` (wry `focus_parent`) on every terminal focus, or keystrokes silently go to the web page. **Key equivalents:** GPUI overrides `performKeyEquivalent:` on the parent view, so cmd-c/cmd-v/cmd-a/cmd-z in web inputs may be swallowed or never reach the subview. **Unverified, must test**; it may need a GPUI patch, and the GPUI rev is pinned. **Mouse:** GPUI's tracking area covers the whole view, so it may keep reporting hover/cursor over the pane beneath (cursor flicker). **Hidden sessions:** `set_visible(false)` on switch; each WKWebView brings WebContent processes (tens to 100+ MB, unverified). |
| **(b) wry** | Is (a) with less objc code | Same pitfalls. Adds wry + its WebKit bindings. wry is built for tao/winit hosts; GPUI as host is untested territory. |
| **(c) Separate native window per session** (an NSWindow with a WKWebView via objc2, or wry on its own window; optionally `addChildWindow:` to stick to fuxx's window) | "This session's browser" opens beside fuxx | No z-order, clipping or key-equivalent fights: it's its own window with its own responder chain. Not inside a pane, so it doesn't feel as integrated. Window management (position, restore) is extra. **M.** |
| **(d) System browser** | `open URL` (what cmd-click does today, M5) | Zero cost. Agents already drive real browsers via Playwright MCP / Chrome DevTools MCP / Claude in Chrome. |

### 4.3 What cmux's browser does

- A WKWebView in a split ([cmux browser docs](https://mintlify.wiki/manaflow-ai/cmux/features/browser), third-party
  docs mirror, unverified).
- Opened with a key, `cmux new-pane --type browser`, or a split button.
- **Scriptable for agents:** an API "ported from agent-browser": `cmux browser snapshot` (accessibility tree as
  JSON), `click --ref`, `fill --ref --value`, `eval`.
- Shared cookies (one `WKProcessPool`), devtools in a split, HTTP blocked except localhost, terminal links open in
  it by default.
- Remote workspaces proxy browser traffic through `cmuxd-remote`, so `localhost:3000` on the server just works
  ([SSH docs](https://cmux.com/docs/ssh), verified).
- **So yes: it's an agent tool**, not only a preview. Matching that means building the automation API (snapshot,
  click, fill, eval, plus waits and screenshots). That API is *most* of the work, more than the embedding.

### 4.4 Recommendation

- **Don't build the in-pane browser in M10.**
- The agent case is already served better by Playwright/Chrome DevTools MCP, which work headless, on any browser,
  with a richer API than cmux's.
- The human case ("see my dev server beside the agent") is served by (d) today, or by (c) if wanted.
- The in-pane version fights GPUI on every axis (z-order, focus, key equivalents, clipping, resize) at a pinned
  pre-1.0 rev that every GPUI bump could break.
- If the owner wants something:
  - **(c), size M**: `fuxx browser [TARGET] URL` / "Open link in session browser" opens a per-session window, with
    no automation API (agents keep using MCP browsers).
  - Or (c) plus a small automation API (`browser.eval`, `browser.snapshot` via `evaluateJavaScript`): **L**.
- In-pane (a)/(b): **L with high uncertainty**. Spike it first: one WKWebView over a pane, then test cmd-c/v,
  focus switching, rename-box overlap and live resize.

---

## 5. Should the order change?

- **Keep M10a first among the big items.** It's the most valuable for "run and watch agents": agents survive a GUI
  crash, quit or upgrade. It also changes ids, status ownership and session creation, which M10b and M10c both
  build on.
- **M10b Option A is independent and small (S–M).** It could ship *before* M10a as a quick win, with no rework
  later: an ssh command in a session just moves into the daemon when M10a lands.
  - Doing it first gives a usable SSH feature in days.
  - Doing it after means it gets persistence for free on day one.
  - Either order is defensible; the owner should pick. I lean slightly to **M10a → M10b-A**, because the ssh
    sessions' restore fields can then be written once against the daemon-aware `sessions.json`.
- **M10b Option B depends on M10a** and on a GPUI-free daemon build. Defer it beyond M10 unless the owner works
  mostly on remote hosts (Q4).
- **M10c should drop to "separate window or nothing"**, or move after M11. M11's layout changes would also disturb
  any in-pane native view.

---

## 6. Open questions only the owner can answer (each changes the design)

1. **Q1. Persistence default.** Do sessions keep running after quit **by default** (with a "Quit and End All
   Sessions" item), or only when opted in (`persistent-sessions = on`, as cmux does)? This changes the quit UX,
   whether the daemon is spawned at all for most users, and the M7 restore wording.
2. **Q2. With the window closed, should fuxx still post notifications** when an agent waits or finishes? Yes means
   the notifier runs in the daemon too.
3. **Q3. With the window closed, must `fuxx` / `fuxx mcp` keep working** (agents orchestrating agents headless)?
   Yes means the daemon becomes the M9 endpoint inside M10a (+M), with GUI-only methods returning `no_window`. No
   means M9 stays in the GUI.
4. **Q4. How much of your agent work runs on remote machines?** Mostly local → M10b Option A only. Mostly remote,
   with flaky links → Option B is worth its XL cost, and M10a's daemon should be built as a library from the start.
5. **Q5. Who is the browser for?** You previewing a dev server → (c) separate window or (d) nothing. Agents
   automating the visible page (cmux-style) → (c) or (a) plus an automation API; and is that worth an L when MCP
   browsers exist?
6. **Q6. If the TCC spike fails** (sessions lose `~/Documents`/`~/Desktop` access after fuxx quits): accept a
   documented limit, or hold M10a until M12 signing gives the daemon a stable identity?

---

## 7. Sources

Official docs and source (verified today):
- tmux man page: https://man.freebsd.org/cgi/man.cgi?query=tmux
- tmux Control Mode: https://github.com/tmux/tmux/wiki/Control-Mode
- WezTerm multiplexing: https://wezterm.org/multiplexing.html
- WezTerm codec (CODEC_VERSION, PDUs): https://raw.githubusercontent.com/wezterm/wezterm/main/codec/src/lib.rs
- cmux SSH: https://cmux.com/docs/ssh
- cmux local persistence issue / PR: https://github.com/manaflow-ai/cmux/issues/10651 · https://github.com/manaflow-ai/cmux/pull/10657
- zmx: https://github.com/thinety/zmx
- dtach: https://dtach.sourceforge.net/ · abduco: https://www.brain-dump.org/projects/abduco/
- wry (Context7 `/tauri-apps/wry`): `build_as_child`, `set_bounds`, `focus_parent`
- GPUI at 1d2a4b3: `crates/gpui/src/window.rs` (6138 `HasWindowHandle`, 4217 `paint_surface`), `crates/gpui_macos/src/window.rs` (141–288 event overrides, 950–985 view setup, 1916 AppKit handle, 2624 `make_backing_layer`), `crates/remote/src/transport/ssh.rs` (ControlMaster, `-t`)
- alacritty_terminal 0.26.0: `grid/mod.rs:109-117`, `term/mod.rs:359,458,489`

Third-party or search summaries (unverified):
- cmux browser docs mirror: https://mintlify.wiki/manaflow-ai/cmux/features/browser
- cmux persistence discussion: https://github.com/manaflow-ai/cmux/discussions/681
- zellij resurrection: https://zellij.dev/documentation/session-resurrection
- kitty sessions: https://sw.kovidgoyal.net/kitty/sessions/ · changelog https://sw.kovidgoyal.net/kitty/changelog/
- WezTerm codec mismatch report: https://github.com/wez/wezterm/issues/3141
- TCC responsible process: https://qt.io/blog/the-curious-case-of-the-responsible-process · https://developer.apple.com/forums/thread/805245
- Claude Code bg-pty-host TCC bug: https://claudeissues.com/issue/59065-bug-bg-pty-host-children-blocked-by-macos-tcc-on-documents-despite-fda-grant-fai
- SMAppService: https://developer.apple.com/documentation/servicemanagement/updating-helper-executables-from-earlier-versions-of-macos · https://developer.apple.com/forums/thread/779379
- iTerm2 CVE-2026-41253: https://lilting.ch/en/articles/iterm2-cve-2026-41253-ssh-conductor-pty-confusion
- Warp subshells: https://docs.warp.dev/terminal/warpify/subshells
- tmux OSC passthrough: https://github.com/ferologics/pi-notify · https://pkg.go.dev/github.com/soloterm/tnotify@v0.1.1
- VS Code Remote-SSH: https://code.visualstudio.com/docs/remote/ssh (from memory)
