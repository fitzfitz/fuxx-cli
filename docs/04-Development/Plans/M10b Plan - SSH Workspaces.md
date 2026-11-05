---
tags: [development, plan, milestone]
milestone: M10b
status: built
updated: 2026-11-06
---

# M10b Plan — SSH Workspaces (local `ssh`, host picker, reconnect)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[51 - Milestone 10b - SSH Workspaces]] (the spec) · design evidence: [[Input - M10 Feasibility Study]] §3.3 |
| **Status** | 🟨 Built and reviewed (2026-11-06); on-screen check pending |
| **Tasks done** | 8 / 9 (Task 8 is your on-screen check, batched with M10c and M11) |
| **▶ Pick up here** | Task 8, when you run the batched checks ([[51 - Milestone 10b - SSH Workspaces]], "On-screen checklist") |
| **Decisions used** | You skipped the questions ("so"). Design = the study's Option A, AI's call throughout. |
| **Needs your eyes** | 👀 Task 8 (batched with M10c and M11, your choice 2026-11-05) |

**Goal:** An SSH session is a fuxx session whose program is the Mac's own `ssh -t HOST`, opened from a host
picker, grouped by host, reusing one connection per host, holding its screen with a "press Enter to reconnect" line
when the link drops, and saved/restored like any session — in Local and background-service (M10a) modes.

**Architecture:** Pure modules for the `ssh` command line (`ssh_target.rs`), the host list (`ssh_hosts.rs`) and the
shared-connection folder (`ssh_control.rs`). The launch path gains a real program (argv) and an exit code, in
Local mode and over the M10a protocol (version 2). The workspace adds an SSH field to sessions, host groups, the
disconnected state and reconnect. A new in-window picker (`host_picker.rs`) and the `fuxx`/MCP `--ssh` option are
the ways in.

**Tech Stack:** No new crates. `std::process::Command` for `ssh -G` / `ssh -O check`, `portable-pty 0.9.0`
`CommandBuilder::new(program)` + `.args()` and `Child::try_wait/wait` → `ExitStatus::exit_code()` (verified in the
registry source), serde/serde_json, GPUI at the pinned rev.

**Spec:** [[51 - Milestone 10b - SSH Workspaces]] · [[12 - Scope]]

## Global Constraints

- The host goes to `ssh` as **one argv element**, never through a shell. Refuse an empty host, a leading `-`,
  whitespace or control characters.
- ssh is run as the pane's **program** (argv), not typed into a login shell.
- Pass `-o ControlMaster=auto -o ControlPath=<dir>/%C -o ControlPersist=10m` **only** when the host's `ssh -G`
  output shows `controlmaster` unset (`false`/`no`); pass `-o ServerAliveInterval=20 -o ServerAliveCountMax=2`
  **only** when `serveraliveinterval` is `0`.
- Shared-connection folder: `/tmp/fuxx-<uid>` (short, because macOS socket paths max out at 104 bytes and ssh adds
  ~17 characters), created 0700, used only if it is a real directory owned by us with mode 0700.
- Disconnected means ssh exit code **255**. Any other exit closes the session as today.
- Banner text, exact: `Disconnected from HOST — press Enter to reconnect` (with tmux, while waiting:
  `Disconnected from HOST — reconnecting in Ns, or press Enter`).
- Auto-reconnect waits: 3, 6, 12, 24, 48, then 60 s, tmux sessions only, while the window is open.
- tmux remote command: `tmux new -A -s fuxx-<name>`, with `<name>` reduced to `[A-Za-z0-9_-]`, at most 32 chars,
  `session` when empty.
- Hidden git hosts: effective `user git`, or effective hostname `github.com`, `gitlab.com`, `bitbucket.org`,
  `ssh.dev.azure.com`, `vs-ssh.visualstudio.com`.
- Shortcut: **⇧⌘S** for File → "New SSH Session…".
- Off mode and non-SSH sessions behave exactly as today; every existing test passes unchanged.
- Protocol: `PROTO_MAX` becomes 2; a version-1 daemon keeps working for non-SSH sessions; an SSH session against a
  version-1 daemon is refused with `restart the background service to use SSH sessions (fuxx daemon stop)`.

## Review Focus

1. **A host string that would inject options or shell code** (`-oProxyCommand=…`, a newline) → refused before any
   process starts (Task 1 test).
2. **The user's own ControlMaster/keepalive config is overridden** → fuxx's `-o` options are left out when `ssh -G`
   shows them set (Task 1 test with recorded `ssh -G` output).
3. **A dead shared connection hangs every new pane to that host** → `ssh -O check` with a 2 s cap before reuse; on
   failure the stale socket is removed (Task 3 test with a fake `ssh`).
4. **An SSH error vanishes** (pane closes before you can read why) → exit 255 holds the pane; tested at the decision
   function and with a real child exiting 255 through Local and daemon backends (Tasks 4, 5).
5. **A version-1 background service silently runs a login shell instead of ssh** → refusal message (Task 4 test).

---

## Files

- Create: `src/ssh_target.rs`, `src/ssh_hosts.rs`, `src/ssh_control.rs`, `src/host_picker.rs`.
- Modify: `src/pty.rs`, `src/terminal.rs`, `src/terminal_view.rs`, `src/mux_proto.rs`, `src/daemon_session.rs`,
  `src/daemon.rs`, `src/daemon_client.rs`, `src/workspace.rs`, `src/workspace_daemon.rs`, `src/groups.rs`,
  `src/restore.rs`, `src/adopt.rs`, `src/workspace_api.rs`, `src/cli.rs`, `src/mcp.rs`, `src/main.rs`.

### Task 1: The `ssh` command line — ✅

**Files:** Create `src/ssh_target.rs` (+ `mod` in `main.rs`).

**Interfaces — Produces:**
- `#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)] pub struct SshTarget { pub host: String, #[serde(default)] pub tmux: bool }`
- `pub fn validate_host(host: &str) -> Result<String, String>` — trims; errors on empty, leading `-`, any whitespace or control char inside.
- `#[derive(Default, Clone, Debug, PartialEq)] pub struct Effective { pub user: Option<String>, pub hostname: Option<String>, pub has_control_master: bool, pub has_keepalive: bool }`
- `pub fn parse_effective(ssh_g_output: &str) -> Effective` — lowercase `key value` lines; `has_control_master` = `controlmaster` not in {`false`,`no`}; `has_keepalive` = `serveraliveinterval` ≠ `0`.
- `pub fn is_git_host(e: &Effective) -> bool` — the Global Constraints list.
- `pub fn tmux_session_name(name: &str) -> String`
- `pub fn ssh_argv(t: &SshTarget, name: &str, e: &Effective, control_dir: Option<&Path>) -> Vec<String>` — `["ssh", "-t", <-o options per constraints>, "--", host]` + (tmux) `["tmux", "new", "-A", "-s", "fuxx-" + tmux_session_name(name)]`. ControlMaster options only when `control_dir` is `Some` and `!e.has_control_master`.

- [ ] **Step 1: failing tests** — `validate_host_refuses_options_and_control_text` (`""`, `"-oProxyCommand=x"`, `"a b"`, `"h\nx"`, `"h\tx"` refused; `"h;rm"` allowed, since it is one argv element and no shell sees it; `" user@host "` → `"user@host"`); `parse_effective_reads_recorded_output` (two recorded `ssh -G` fixtures: plain defaults, and one with `controlmaster auto` + `serveraliveinterval 30`); `git_hosts_are_hidden` (user git; hostname github.com; a normal host not); `tmux_names_are_safe` (`"my agent!"` → `"my_agent_"`; 40 chars → 32; `""` → `"session"`); `argv_adds_reuse_and_keepalive_only_when_unset` (exact vectors for both fixtures, with and without tmux, `--` always right before the host).
- [ ] **Step 2:** `cargo test ssh_target` → FAIL (module missing).
- [ ] **Step 3:** implement.
- [ ] **Step 4:** `cargo test ssh_target` → PASS; full `cargo test` green.
- [ ] **Step 5:** commit `feat(m10b): the ssh command line, built from argv with reuse and keepalive unless configured`.

### Task 2: The host list — ✅

**Files:** Create `src/ssh_hosts.rs`.

**Interfaces — Produces:**
- `pub fn config_hosts(config: &Path, home: &Path) -> Vec<String>` — `Host` names in file order, following `Include` (relative to `home/.ssh`, `~` expanded, simple `*` globs, depth ≤ 8, cycles ignored), skipping names containing `*` or `?` or starting with `!`; case-insensitive keywords; `Host a b` gives both; duplicates dropped.
- `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct Recent { pub target: SshTarget, pub used_at: u64 }`
- `pub fn load_recent(path: &Path) -> Vec<Recent>` / `pub fn record_recent(path: &Path, target: &SshTarget, now: u64) -> io::Result<()>` — newest first, one entry per host (latest wins), at most 10; file `~/Library/Application Support/fuxx/ssh-recent.json`; a broken file reads as empty.
- `pub fn picker_rows(recent: &[Recent], config: &[(String, Option<Effective>)]) -> Vec<SshTarget>` — recent first, then config hosts not already listed and not `is_git_host` (unresolved hosts are shown).
- `pub fn resolve(host: &str, timeout: Duration) -> Option<Effective>` — runs `ssh -G -- host` (stdin null), parses with `parse_effective`; `None` on error or timeout (callers pass 2 s). Program from the `FUXX_SSH` env var if set (tests), else `ssh`.

- [ ] **Step 1: failing tests** (temp dirs): `hosts_follow_includes_and_skip_patterns`, `include_cycles_and_depth_are_bounded`, `recent_is_newest_first_deduped_and_capped`, `broken_recent_file_reads_empty`, `picker_rows_put_recent_first_and_hide_git_hosts`, `resolve_uses_a_fake_ssh` (temp script printing a fixture, via `FUXX_SSH`), `resolve_times_out` (script sleeping 5 s, timeout 200 ms → `None` in < 1 s).
- [ ] **Steps 2–4:** FAIL → implement → PASS; full suite green.
- [ ] **Step 5:** commit `feat(m10b): host list from ~/.ssh/config and recent hosts`.

### Task 3: The shared-connection folder and health check — ✅

**Files:** Create `src/ssh_control.rs`.

**Interfaces — Produces:**
- `pub fn control_dir_in(base: &Path, uid: u32) -> io::Result<PathBuf>` — `base/fuxx-<uid>`; creates it 0700; refuses (Err) a symlink, another owner, or a mode other than 0700. `pub fn control_dir() -> Option<PathBuf>` = `control_dir_in("/tmp", getuid())`, logging one `[fuxx] ssh:` line and returning `None` on error (then no reuse options are passed).
- `pub fn ensure_healthy(host: &str, dir: &Path, timeout: Duration)` — run `ssh -o ControlPath=<dir>/%C -O check -- host` (program via `FUXX_SSH`); on failure or timeout (callers pass 2 s), run `-O exit` (1 s cap) and remove any socket file in `dir` that refuses a connection. Called only on background threads.

- [ ] **Step 1: failing tests:** `control_dir_is_private` (created 0700), `control_dir_refuses_symlink_and_wrong_mode`, `a_failed_check_removes_the_stale_master` (fake ssh exiting 255 on `-O check`; a dead socket file in the dir is removed), `a_hung_check_is_capped` (fake ssh sleeping; returns within ~3.5 s).
- [ ] **Steps 2–4:** FAIL → implement → PASS; full suite green.
- [ ] **Step 5:** commit `feat(m10b): private shared-connection folder with a health check before reuse`.

### Task 4: A real program and an exit code, Local and background service — ✅

**Files:** Modify `pty.rs`, `terminal.rs`, `terminal_view.rs`, `mux_proto.rs`, `daemon_session.rs`, `daemon.rs`, `daemon_client.rs`, `workspace_daemon.rs`.

**Interfaces — Produces:**
- `pty::command(program: Option<&[String]>, cwd: Option<&Path>, env: &ShellEnv) -> CommandBuilder` — `None` = today's `shell_command`; `Some(argv)` = `CommandBuilder::new(&argv[0])` + `.args(&argv[1..])`, same TERM/COLORTERM/env.
- `TermEvent::Exited { code: Option<u32> }` (was a unit variant; every match updated). Local: after reader EOF, wait for the child on the reader thread → `code`.
- `TerminalView::spawn` and the daemon create path take `program: Option<Vec<String>>`; `startup` is ignored when a program is given.
- Protocol 2: `PROTO_MAX = 2`; `Create.program: Option<Vec<String>>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`); `CreateSpec::from` keeps it; new frame `ExitCode { id: u64, code: Option<u32> }` (new kind byte), sent right before `Exited`, only on connections negotiated at ≥ 2. The replica combines them into `TermEvent::Exited { code }`.
- `DaemonClient::proto(&self) -> u32`. A create with a program on proto 1 → `Err` with the Global Constraints message.

- [ ] **Step 1: failing tests:** `pty_runs_argv_without_a_shell` (`["/bin/sh","-c","exit 255"]` → `Exited { code: Some(255) }`; `["/bin/echo","a;b"]` prints `a;b`), `a_plain_shell_exit_has_its_code`, `exit_code_frame_round_trips`, `negotiate_2_and_1_both_work`, `daemon_reports_the_exit_code_before_exited` (in-process daemon, program exiting 255), `proto_1_daemon_refuses_a_program`.
- [ ] **Steps 2–4:** FAIL → implement → PASS; full suite green; existing tests unchanged apart from the `Exited` pattern.
- [ ] **Step 5:** commit `feat(m10b): sessions can run a program directly and report its exit code (protocol 2)`.

### Task 5: SSH sessions in the workspace: launch, groups, disconnect, reconnect, restore — ✅

**Files:** Modify `workspace.rs`, `workspace_daemon.rs`, `groups.rs`, `restore.rs`, `adopt.rs`, `terminal.rs`.

**Interfaces:**
- Consumes Tasks 1–4.
- `TerminalCore::show_local(&mut self, text: &str)` — parses text into the screen only; nothing reaches the child.
- `Session.ssh: Option<SshTarget>`; `SavedSession.ssh: Option<SshTarget>` (`serde(default, skip_serializing_if = "Option::is_none")`); `SavedState.collapsed_hosts: Vec<String>` (`serde(default)`). No `version` bump (additive).
- `pub enum GroupKey { Folder(PathBuf), Host(String) }`; `groups::{group_label, insert_index, rows}` take `Option<&GroupKey>` and `collapsed: &[GroupKey]`. A host group's label is the host. Ungrouped first, then groups in first-seen order, as today.
- `pub enum ExitAction { Close, Hold }`; `pub fn on_exit(ssh: Option<&SshTarget>, code: Option<u32>) -> ExitAction` — `Hold` only for SSH + `Some(255)`.
- `pub fn reconnect_delay(attempt: u32) -> Duration` (3, 6, 12, 24, 48, 60, 60 …) and `pub fn disconnect_banner(host: &str, next: Option<u64>) -> String` (exact Global Constraints texts, wrapped in `\r\n`).
- `Workspace::open_ssh(&mut self, target: SshTarget, place: Place, window, cx)` (`Place` = how the existing opens choose index/leaf: new, split of a pane, or replace an index) — validates, then on a background thread `resolve` + `control_dir` + `ensure_healthy`, then opens with `ssh_argv` as the program, project `None`, `ssh: Some(target)`, name = host; records the recent host.
- ⌘T / split / a group's "+" while the active (or the group's) session is SSH → `open_ssh` with the same target.
- On `Exited { code }`: `Close` → today's `remove_at`; `Hold` → keep the session, `show_local(banner)`, status Done, mark it disconnected; tmux → schedule auto-reconnect with `reconnect_delay`. Enter on a disconnected session (consumed, not sent) or the timer → reconnect: `open_ssh` with `Place::Replace(index)` (same index and layout leaf, name and group), then drop the old. Closing the pane cancels the timer.
- Restore: a saved entry with `ssh` reopens via `open_ssh` in `Layout` and `Agents` modes, not in `Off`. `adopt::place` keeps `ssh` from the saved entry.

- [ ] **Step 1: failing tests:** `groups_key_by_folder_or_host` (labels, insert order, collapsed hosts), `on_exit_holds_only_ssh_255`, `reconnect_delays`, `banner_texts`, `saved_ssh_round_trips_and_old_files_load`, `restore_reopens_ssh_in_layout_and_agents_not_off`, `adoption_keeps_the_ssh_target`, `show_local_writes_to_the_screen_only`.
- [ ] **Steps 2–4:** FAIL → implement → PASS; full suite green.
- [ ] **Step 5:** commit `feat(m10b): ssh sessions open, group by host, hold on disconnect, reconnect and restore`.

### Task 6: The host picker and the menu item — ✅

**Files:** Create `src/host_picker.rs`; modify `workspace.rs` (overlay, action), `main.rs` (action `NewSshSession`, File menu item "New SSH Session…" after "New Session in Folder…", binding `cmd-shift-s`, key context `HostPicker`).

**Interfaces:**
- `HostPicker` entity: a centred panel over the workspace with a text box (reuse `NameInput`'s text field if it fits as is; otherwise a sibling with the same IME handling), the rows, and a "Keep running on the host (tmux)" checkbox.
- Pure `pub fn filter_rows(rows: &[SshTarget], query: &str) -> Vec<PickerRow>`, `pub enum PickerRow { Host(SshTarget), Typed(String) }` — case-insensitive substring match; a non-empty query that isn't exactly a row's host adds `Typed(query)` last ("Connect to …"); a query failing `validate_host` adds none.
- Pure `pub fn move_selection(current: usize, len: usize, delta: i32) -> usize` — wraps.
- Keys: ↑/↓ move, Enter opens, ⌥Enter opens with tmux, Tab toggles the checkbox, Escape or blur closes. A recent row carries its own tmux choice; the checkbox overrides it.
- Emits `HostPickerEvent::{Open(SshTarget), Cancel}`; the workspace calls `open_ssh` (new place) and refocuses.
- Config hosts resolve on a background thread when the picker opens; rows update as they resolve (git hosts drop out); typing is never blocked.

- [ ] **Step 1: failing tests:** `filter_rows_matches_and_offers_typed_host`, `filter_rows_never_offers_an_invalid_host`, `selection_wraps`.
- [ ] **Steps 2–4:** FAIL → implement → PASS; full suite green. GUI behaviour is checked in Task 8.
- [ ] **Step 5:** commit `feat(m10b): host picker (⇧⌘S) and the File menu item`.

### Task 7: `fuxx new --ssh` and the MCP tool — ✅

**Files:** Modify `cli.rs`, `workspace_api.rs`, `mcp.rs`.

**Interfaces:** `fuxx new --ssh HOST [--tmux] [--name N] [--split right|down --of sN] [--background]`. `--ssh` conflicts with `--folder`, `--preset` and `--run` (usage error naming the clash; `--tmux` without `--ssh` is an error). `session.new` param `ssh: { host, tmux }`; a bad host returns `bad_request` with the `validate_host` message. MCP `new_session` gains `ssh_host` (string) and `tmux` (bool). `session.list` adds `"ssh": "<host>"` for SSH sessions only.

- [ ] **Step 1: failing tests:** `new_ssh_parses_and_conflicts`, `api_new_ssh_validates_host`, `mcp_new_session_maps_ssh`, `list_shows_ssh_host_only_for_ssh`.
- [ ] **Steps 2–4:** FAIL → implement → PASS; full suite green.
- [ ] **Step 5:** commit `feat(m10b): fuxx new --ssh and the MCP ssh option`.

### Task 8: On screen — 👀 pending (batched)

Run with the M10c and M11 checks. The full merged list is in [[51 - Milestone 10b - SSH Workspaces]] ("On-screen checklist"); the list below is the plan's original.

The spec's success check, plus:
- [ ] The picker lists your config hosts (git hosts hidden), recent ones first; typing a new host offers "Connect to …"; Escape closes.
- [ ] Two panes to one host: the second opens without a second login. A wrong host shows ssh's error with the reconnect line instead of vanishing.
- [ ] Wi-Fi off → the line appears within about a minute; Enter reconnects in the same place. With tmux, it reconnects by itself.
- [ ] ⌘T on an SSH session opens another to the same host. The sidebar groups by host.
- [ ] Restore and M10a: quit/reopen keeps SSH sessions (persistence on), or reconnects them (persistence off, `session-restore = agents`).
- [ ] `fuxx new --ssh HOST` works and `fuxx ls` shows the host.

### Task 9: Update the docs — ✅

- [x] Milestone note 51 (🟨 built, check pending, not ✅); this plan + verification log; roadmap; tracker; Progress Log; STRUCTURE.md (new files); README (an "SSH sessions" paragraph).

## Verification log

Every task was built test-first and reviewed (a fresh reviewer per task); test counts are the full `cargo test`
total after the task. The branch started from 399 tests. `cargo fmt --check` clean.

| Task | Commits | Tests | Review |
|---|---|---|---|
| 1 ssh_target | `f59167f..1ea7919` | 404 | Approved, clean. |
| 2 ssh_hosts | `1ea7919..af27985` | 411 | Approved, clean. |
| 3 ssh_control | `af27985..d8ececa` | 416 | Approved, clean. |
| 4 program and exit code (protocol 2) | `d8ececa..247a8d4` | 424 | Approved, clean (all 3 named risks pass). |
| 5 workspace SSH | `247a8d4..a361fee` | 438 | Needs fixes (2 Important, plus 2 re-graded Minors and the per-pane tmux name); fix round `36b5a25` (move to `workspace_ssh.rs`), `335d1a4` (flaky fake-ssh tests), `a361fee`; scoped re-review: 6 addressed, 0 open. |
| 6 host picker | `a361fee..a6c322c` | 442 | Approved, clean. |
| 7 `fuxx new --ssh`, MCP | `a6c322c..58fc45d` | 447 | Approved with one Important (`fuxx ls` did not show the host); fixed in `58fc45d`, re-reviewed clean. |
| Final review | `2a72ce6..58fc45d` | 447 | Ready after fixes (0 Critical, 3 Important, 12 Minor). Important: SSH panes lacked the login-shell environment; early ssh failures vanished; retries never reset or stayed noisy. |
| Final fixes | `b094f75`, `ca215c1` | 457 | 10 new tests, each shown failing first. Also fixed: `#` comments in `Host` lines, auto-reconnect stealing focus, a hung shared connection never cleared. Scoped re-review: all addressed (6 of 6), no new Critical or Important. |
| 8 On screen | | | Pending: batched with M10c and M11. |
| 9 Docs | the docs commit | 457 | This commit. |

Rulings taken along the way are summarised in the milestone note, "Decisions made along the way".

## Deferred minors

Minor review findings left on purpose. Copied here from the (git-ignored) working ledger so they are not lost.
Items marked "(fixed later)" were fixed in a later round and are kept for the record.

- **Task 1:** is_git_host hostname match is case-sensitive; ControlPath built with display() (lossy/'%' expansion, harmless for /tmp/fuxx-<uid>); untested controlmaster variants (no/yes/ask) and " -x". (case-insensitive match fixed in Task 2, Ruling C)
- **Task 2:** `Host foo # work` adds "#"/"work" as hosts; globs only '*' in the last component; temp test dirs not cleaned; dead-code comment sits above mod ssh_hosts (cosmetic). (the `#` comment case fixed in `b094f75`)
- **Task 3:** run_capped kills only the direct child (a grandchild holding stdout keeps the reader thread); the 1 s stdout wait also applies to -O check (theoretical); temp test dirs not cleaned; to_string_lossy on the control path.
- **Task 4:** hang_up killpg can run on an already-reaped pid within one reap tick (documented; pid-reuse race only when the group is empty); a child that closes its terminal but keeps running reports Exited ≤1 s late with code None; signal deaths read as Some(1).
- **Task 5:** a rename during a reconnect is lost (name captured at reconnect start); waits stop growing on a black-holed network (opened_at includes connect time; fixed later by the "was up" rule in the final fixes); Local restore gap lets a ⌘T during async SSH restore drop saved splits; hold/reconnect/replace have no automated test (GPUI).
- **Task 5:** a live-adopted session saved before tmux names existed starts a new tmux session on first reconnect (unreleased branch only); tmux name 24 random bits; screen-reset test polls 5 s; failed tries still emit status.changed Idle→Done to scripts (notification suppressed); banner shows the wait once, no countdown; daemon newcomers not in sessions.json come back as plain sessions (List has no host).
- **Task 6:** typing a host that is a substring of a recent one selects the recent one first (Enter opens it; "Connect to" is last, per brief) — put in the Task 8 checklist; placeholder uses fixed offsets; recent/config files read on the UI thread at open (small); IME + ↑/↓/Tab unverified.
- **Task 7:** empty folder/preset "" counts as unset in the API but as a clash in MCP/CLI; --name applied via rename after open (brief automatic name); no test for the API's ssh+folder/preset/run clash (inline in GPUI-bound api_new).
- **Final:** an explicit `ControlMaster no` in ssh config is indistinguishable from the default and gets fuxx's reuse options; other final-review minors as listed in final-review.md.
- **Final:** login-env failure is cached for the run (one slow rc at first launch → no login env until restart); a hung master process may outlive its removed socket; adopted daemon session's first_output only set on next output; config_hosts doc wording.
- **Final (from the final review):** a reconnect try shows a blank pane until ssh says something; a tmux pane closed with ⌘W leaves its random-named remote tmux session behind; `session.list` reports a held session as live with no "disconnected" flag; `record_recent` is load-modify-write with no lock, so two opens at once can drop one entry; an adopted live SSH session that ends in the second before its lazy attach shows as "not live" instead of held; restore with several SSH sessions resolves them one after another (up to about 5 s each in the worst case), so the window before splits are placed can last seconds.

## Known limitations

- No remote fuxx service: remote `fuxx`/MCP and persistent remote sessions without tmux are a later milestone.
- Notification codes inside remote tmux need `allow-passthrough on` (the user's tmux config).
- Auto-reconnect runs only while the window is open.
- An explicit `ControlMaster no` is treated like the default (fuxx adds its reuse options).
- A remote shell that exits 255 reads as a disconnect.
- Not run on screen yet (Task 8).

## Related

- [[51 - Milestone 10b - SSH Workspaces]] · [[Input - M10 Feasibility Study]] · [[40 - Roadmap]] · [[00 - Plan Tracker (Start Here)]]
