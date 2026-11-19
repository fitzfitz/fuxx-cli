---
tags: [development, plan, milestone]
milestone: M11q
status: planned
updated: 2026-11-19
---

# M11q Plan — Which command finished

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[68 - Milestone 11q - Which Command Finished]] |
| **Status** | 📝 planned 2026-11-19 on branch `m11q-command-finished`; not built |
| **Tasks done** | 0 / 11 (plus Task 0) |
| **Baselines** | Task 0 fills this in (expected clippy: default 14 bin / 27 test; `ui-shots` 16 / 29) |
| **▶ Pick up here** | Task 0 |
| **Decisions used** | Yours: the pane is named after the running command; the notification says which command; after M11p; "go ahead" (2026-11-19, incl. the row's who `› <short form>`); the full ui-shots run only before the final review and after its fix wave. The design's AI's calls §1–§6. Plan-time calls below (AI's). |
| **Needs your eyes** | 👀 the plan-time calls 1, 2, 5 and 6 (they rule on things the design left open); then the on-screen check in note 68, after Task 11 |

**Goal:** While a command started at the shell prompt runs, its pane is named after its short form; when it ends, the
banner, the 🔔 list and Activity say which command it was and how it ended, and nothing but the short form ever
leaves the terminal.

**Architecture:** A pure `src/command_name.rs` makes the short form, the ending words and keeps "what runs now"
(`CommandRun`). `blocks::advance` returns the OSC 133 marks it applied as `CommandMark`s **already reduced to the
short form**, so the full line never enters an event. Both terminal backends send them as `TermEvent::Command` past
M11n's Output gate; the view turns them into `TerminalEvent::Command(Change)`; the Workspace
(`src/workspace_command.rs`) keeps `Session::command`, settles the status at the end mark and words the banner, the
🔔 entry and the Activity entry. The background service does the same for its own tracker and its own banners. No
protocol change.

**Tech Stack:** Rust 2024, GPUI (pinned rev, unchanged), `alacritty_terminal`. **No new crates, no new GPUI API**
(if one turns out to be needed, check Context7 or the pinned source first; never a remembered API).

**Spec:** [[68 - Milestone 11q - Which Command Finished]] (binding), with its "Update 2026-11-19". Read
[[77 - Milestone 11p - Chrome Caching]] for the chrome rule.

## Plan-time calls (AI's, made while reading the code on 2026-11-19)

1. **The banner keeps today's shape (M11v part 5), not the design's 2026-11-10 one.** Banners are now title
   `<workspace> — <message>`, subtitle `#2 · <group> · <agent>`, body. The command's short form replaces the
   *message*: title `fuxx-cli — pnpm build`, subtitle `#2`, body `Done in 3m 12s`. The design's
   `#2 fuxx-cli — pnpm build` is the same information in the older layout.
2. **No minimum length for an end-mark banner; the silence guess keeps its 10 s** (`notifier::QUIET_UNDER`, added
   after the design). So "one banner per ending" is kept by two per-session flags, not by the status alone: at an
   end mark with the status already Done, the result goes to the 🔔 list once, and a banner goes out **only if this
   Done had none** (it was under 10 s, or you were looking then). The pure table is `notifier::end_tells` (Task 7).
3. **The Workspace settles at the end mark for both backends**, by the same pure rule the trackers use
   (`status::settles_on_end`): Working or Waiting → Done; Idle → Done only when the command ran ≥ `IDLE_AFTER`
   (3 s); Done stays. The tracker's (or the service's) own `Done` then arrives as a no-change. The window and the
   service each time the command with their own clock; a command within a few ms of 3 s from Idle can settle in one
   and not the other (then the usual `Finished` banner, or a Done the next status frame corrects). Stated in Limits.
4. **After an end mark, output is prompt noise until the next keystroke** (`last_input = None`, as at startup).
5. **A start mark re-arms Working only after a keystroke since the previous start mark.** Typed ahead while the last
   command ran: the next command works and gets its own banner. With no keystroke between two starts (several pasted
   lines; a file's forged marks) the second start names the pane but re-arms nothing, so forged marks are held to
   one banner per keystroke, as OSC 9 notifications are today. Stated in Limits.
6. **Short-form details the design leaves open:** the subcommand is the word **right after** the program or none
   (`curl -H …` → `curl`); letters and digits are ASCII; the program's last path part has control characters
   dropped and is cut at 32 characters (`PROGRAM_CAP`); a program starting with `$`, `(` or a backtick, or an
   unclosed quote **anywhere** in the line: no short form (`agent_detect::program`'s rule); `&&`, `||`, `;`, `|`
   and a newline count outside quotes only, also glued to a word (`sleep 8; true`), and only with something after
   them; a lone `&` and redirections end the reading without ` …`; a wrapper with nothing after it is the program
   (`sudo -i` → `sudo`). By the design's rule a wrapper option's value reads as the program (`sudo -u deploy make`
   → `deploy make`): a Limit.
7. **Durations:** the two largest units, the smaller left out when zero: `40s`, `3m 12s`, `2m`, `1h 4m`, `1h`,
   `1d 2h`.
8. **An unnamed workspace's row line one follows `Session::label`** (as it does for a program's title today), so it
   reads the short form too; line two's who reads `› <short form>` (the 2026-11-19 update).
9. **The name ends** at the end mark, a prompt mark, a reattach snapshot (`CommandMark::Unknown`), an SSH hold and a
   lost service connection. A snapshot never starts one.
10. **The service's own banner** (no window attached): summary `fuxx — <short form>` (else `command`), body the
    ending words; a silence banner while a command runs names it through `auto_name`. Its command state is read
    from the session (`DaemonSession::command`, `take_ended`), not sent: `SessionInfo` is unchanged.
11. **Tests post nothing:** banners go through one `Workspace::post_banner`, which under `ui-shots` records the
    three strings (`shot_banners`) instead of calling `notifier::fire` and the sound. The service's banner words are
    a pure function.
12. **Copy** (exact): `Done in {d}`, `Stopped after {d}`, `Failed (exit {n}) after {d}`, `Finished after {d}`,
    `command`; 🔔 list `{name} · {words}` (U+00B7); Activity `{name} — {words}` (U+2014); ` …` is a space and U+2026.

## Global Constraints

- Branch `m11q-command-finished`, main checkout. Commit per task (stage files by path); no merge, no push in the
  build. **No Co-Authored-By or AI attribution lines.**
- **Gate per task:** `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features
  ui-shots && cargo fmt --check` (no new warnings over the baselines: 14 / 27 default, 16 / 29 `ui-shots`), then
  `scripts/ui-shots.sh --changed master`. **The full `scripts/ui-shots.sh` runs only before the final review and
  after its fix wave** (your decision). A failing scene is re-run alone once before it counts. **Never
  `--update-refs`.**
- **M11p:** nothing visible changes inside any `render`; no always-on timer (no `timer(` loop,
  `request_animation_frame` or `spawn` loop added); the chrome always shows current information, and **each new
  source of change gets a test** (the running command starting, ending and being cleared: steps in
  `chrome-session-facts`, Task 6).
- **Only the short form leaves the terminal:** banners, the 🔔 list, the sidebar, tabs, the window title, Activity
  and the scripting API's events never hold more of a command line. No `CommandMark`, `Change`, `TermEvent` or
  `TerminalEvent` carries the full line. Mark text is untrusted: control characters dropped, length capped, escaped
  for AppleScript as titles are (`notifier::applescript_escape`).
- **Provider-specific code only in `src/providers/<agent>.rs`**: nothing here names an agent; `claude` in a test is
  only a command's text.
- **macOS notifications as the code does them today** (`notifier::fire`: native from the bundle, else `osascript`);
  never `notify-rust`. **Tests and scenes must not post a real notification** (plan-time call 11).
- **Not in it** (spec): no change to the silence guess, no bash or fish integration, no Failed status, ring or
  cursor, no more of a command line anywhere, no new scripting API events or fields, no setting.
- **Public repo:** example names are fuxx-cli's own (`fuxx-cli`, `pnpm build`, `cargo test`, the spec's table),
  never other projects', personal paths, time zones or emails. Grep each commit's added lines for `/Users/` before
  it lands.
- **Shell safety:** never run the app by the bare name `fuxx` (the user's real app is on PATH); single-quote grep
  patterns; never put backticks or `$(...)` inside double-quoted shell strings. The app, if ever run, only as
  `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug, never `--release`).
- **Looks:** colours from the theme's tokens (`ui.block_failed`, `ui.block_ok`, `ui.text_faint`), text at `TEXT_MIN`
  and marks at `MARK_MIN` on what they are drawn on; no hex.
- New scenes go in a new area file `src/ui_shots/command.rs` (`FILE` / `SCENES`, a `mod` in `src/ui_shots/mod.rs`,
  an `AREAS` row and names in `registry::ORDER` after the `command-blocks-*` scenes). Agents can't see the window:
  scenes prove behaviour by probes; the look is the user's on-screen check.

## Review Focus

1. **A second command with no keystroke after the first** (typed ahead; several pasted lines): typed ahead, it
   works and banners again; pasted, it is named but adds no second banner. Pinned in Task 5
   (`a_typed_ahead_command_works_again`, `starts_without_typing_do_not_rearm`).
2. **Marks printed by a program** (`cat` of a file with C / D marks, quotes, backslashes, control characters, an
   8 KB command): at most one banner until you type; the name is cut, cleaned and escaped; the real `D` that follows
   finds nothing running and does nothing. Pinned in Task 1 (`control_characters_and_long_names_are_cut`), Task 3
   (`an_end_with_no_start_is_nothing`), Task 5 (above) and Task 7 (`a_command_name_is_escaped_like_a_title`,
   `end_tells`'s `told` row).
3. **A start with no command text** (powerlevel10k's own `C;`, an overlong line) **or an end with no start** (after
   a restart or a reattach): the pane keeps its usual name; the first banners `command`; the second posts today's
   banner or nothing. Pinned in Task 3 (`a_command_with_no_short_form_still_ends`,
   `an_overlong_command_starts_with_no_short_form`) and Task 4 (`a_snapshot_reports_the_command_unknown`).
4. **The pane goes away or is held mid-command** (SSH drop, service connection lost, the shell exits with no `D`):
   the name is cleared, nothing is bannered for the command, no panic. Pinned in Task 4 (scene
   `command-reaches-workspace`'s hold and lost steps).
5. **Unusual endings:** exit 130 from a program that wasn't ⌃C (still `Stopped`), a negative or > 255 exit, a run
   past a day, a run of 0 s. Pinned in Task 2 (`endings_from_the_exit_code`, `durations_use_two_units`).

---

### Task 0: Baselines

- [ ] **Step 1:** On `m11q-command-finished` at the plan commit: `cargo test --bin fuxx` (passed / ignored), both
  clippies (warning counts; expected 14 / 27 and 16 / 29), `cargo fmt --check`. The full `scripts/ui-shots.sh` is
  not run here (your decision): the ui-shots baseline is M11ag's last full run (247 passed, 0 failed). Write the
  numbers into "Baselines".
- [ ] **Step 2:** Commit `M11q Task 0: baselines` (this plan only).

### Task 1: The short form (pure)

**Files:**
- Create: `src/command_name.rs` (module doc: what leaves the terminal; `mod command_name;` in `src/main.rs`)
- Modify: `src/agent_detect.rs` (`is_assignment`, `unquote` become `pub(crate)`; behaviour unchanged)

**Interfaces:**
- Produces: `pub const PROGRAM_CAP: usize = 32; pub const WORD_CAP: usize = 24; pub const NO_SHORT: &str =
  "command";` and `pub fn short_form(line: &str) -> Option<String>`.
- The reading (spec §3, plan-time call 6), in order: (a) split the line into the **first command's** raw words,
  quotes holding spaces, stopping at `&&`, `||`, `;`, `|` or a newline outside quotes (also when glued to a word);
  an unclosed quote anywhere → `None`; remember whether anything non-blank follows the stop; (b) skip leading
  `NAME=value` words; while the word is one of `sudo`, `env`, `time`, `nohup`, `command`, `exec`, skip it
  and then every following word that starts with `-` or is `NAME=value`; if no word is left, the last wrapper
  skipped is the program; (c) the program: the word
  unquoted; empty, or starting with `$`, `(` or a backtick → `None`; its last path part, control characters
  dropped, cut at `PROGRAM_CAP` characters; (d) the next raw word, if plain (ASCII letters, digits, `-_.:`, 1 to
  `WORD_CAP` characters, not starting with `-`); (e) if that word is `run`, `exec`, `x` or `dlx`, the word after
  it, if plain; (f) ` …` when (a) found something after the stop.

- [ ] **Step 1: Write the failing tests** (in `command_name.rs`):
  - `the_specs_table` (each row verbatim): `pnpm dev` → `pnpm dev`; `cargo test --release -- --nocapture` →
    `cargo test`; `API_KEY=sk-123 npm run deploy --token abc` → `npm run deploy`; `sudo -E ./scripts/build.sh prod`
    → `build.sh prod`; `cargo build && cargo test` → `cargo build …`; `curl -H "Authorization: Bearer x"
    https://…` → `curl`; `$(which node) server.js` → `None`.
  - `the_on_screen_checks_commands`: `sleep 5` → `sleep 5`; `sleep 8; true` → `sleep 8 …`; `sh -c 'sleep 4; exit
    3'` → `sh`; `FOO=secret sleep 6 --x` → `sleep 6`; `npm run build` → `npm run build`; `vim` → `vim`; `claude` →
    `claude`.
  - `wrappers_and_their_options_are_skipped`: `env -i PATH=/bin make test` → `make test`; `time cargo build` →
    `cargo build`; `nohup ./run.sh &` → `run.sh`; `command git status` → `git status`; `exec zsh` → `zsh`; `sudo
    -i` → `sudo`; `sudo env FOO=1 nohup make` → `make`.
  - `run_exec_x_and_dlx_take_one_more_word`: `pnpm dlx cowsay hi` → `pnpm dlx cowsay`; `bun x tsc --noEmit` → `bun
    x tsc`; `docker exec -it web sh` → `docker exec`; `npm run` → `npm run`.
  - `only_a_plain_word_follows_the_program`: `export TOKEN=abc` → `export`; `mysql -psecret` → `mysql`; `echo "a &&
    b"` → `echo`; `git aaaaaaaaaaaaaaaaaaaaaaaaa` (25 letters) → `git`; `git é` → `git`.
  - `another_command_adds_an_ellipsis`: `make;ls` → `make …`; `a|b` → `a …`; `ls 2>&1 | less` → `ls …`; a line
    break between two commands → the first and ` …`; `cargo build &&` → `cargo build`; `sleep 5 &` → `sleep 5`.
  - `lines_that_cannot_be_read_have_none`: `""`, `"   "`, `(cd src; make)`, a line starting with a backtick,
    `$EDITOR notes`, `echo "unclosed`, `FOO=1`.
  - `control_characters_and_long_names_are_cut`: a program of 40 letters → its first 32; a program word holding
    `\u{7}` and `\u{1b}` → without them; a result never holds a control character.
- [ ] **Step 2: Run** `cargo test --bin fuxx command_name` — FAIL (module missing).
- [ ] **Step 3: Implement** `short_form` as (a)–(f).
- [ ] **Step 4: Run** — PASS; the gate.
- [ ] **Step 5: Commit** `M11q Task 1: the short form`.

### Task 2: How it ended, in words (pure)

**Files:**
- Modify: `src/command_name.rs`

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Ending { Done, Stopped, Failed(i32), Finished }`;
    `impl Ending { pub fn of(exit: Option<i32>) -> Ending; pub fn failed(self) -> bool }` (`Some(0)` → `Done`,
    `Some(130)` → `Stopped`, any other `Some(n)` → `Failed(n)`, `None` → `Finished`).
  - `pub fn ran_label(d: Duration) -> String` (plan-time call 7).
  - `pub fn ending_words(ending: Ending, ran: Duration) -> String` (plan-time call 12).
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Finished { pub short: Option<String>, pub ending: Ending,
    pub ran: Duration }`; `impl Finished { pub fn name(&self) -> &str` (the short form, else `NO_SHORT`)`; pub fn
    words(&self) -> String` (`ending_words`)`; pub fn listed(&self) -> String` (`"{name} · {words}"`)`; pub fn
    logged(&self) -> String` (`"{name} — {words}"`) `}`.

- [ ] **Step 1: Write the failing tests:**
  - `the_banner_tables_bodies` (the spec's rows): `(Some(0), 192 s)` → `Done in 3m 12s`; `(Some(130), 3840 s)` →
    `Stopped after 1h 4m`; `(Some(101), 40 s)` → `Failed (exit 101) after 40s`; `(None, 120 s)` → `Finished after
    2m`.
  - `endings_from_the_exit_code`: 0 → `Done`; 130 → `Stopped`, not `failed()`; 1, 101, 143, 255, 256, −1 →
    `Failed(n)`, `failed()`; `None` → `Finished`.
  - `durations_use_two_units`: 0 s → `0s`; 8 s → `8s`; 59 s → `59s`; 60 s → `1m`; 192 s → `3m 12s`; 3600 s → `1h`;
    3661 s → `1h 1m`; 3840 s → `1h 4m`; 86 400 s → `1d`; 93 600 s → `1d 2h`.
  - `a_finished_command_in_the_list_and_in_activity`: `pnpm build`, `Failed(101)`, 40 s → `listed()` == `pnpm
    build · Failed (exit 101) after 40s`, `logged()` == `pnpm build — Failed (exit 101) after 40s`; `short: None`
    → `name()` == `command`.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS; the gate.
- [ ] **Step 5: Commit** `M11q Task 2: ending words`.

### Task 3: The marks come out of the parser, and what runs now

**Files:**
- Modify: `src/command_name.rs` (`CommandMark`, `Change`, `CommandRun`), `src/blocks.rs` (`advance` returns the
  marks; module doc), its callers that only drop the result for now: `src/terminal.rs` (×4), `src/daemon_session.rs`,
  `src/vt_snapshot.rs` (×2)

**Interfaces:**
- Produces (`command_name`):
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub enum CommandMark { Started { short: Option<String> }, Ended { exit:
    Option<i32> }, Prompt, Unknown }` (`Unknown`: what runs is not known any more; never made by `advance`).
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub enum Change { Started(Option<String>), Ended(Finished), Cleared }`.
  - `#[derive(Default)] pub struct CommandRun { .. }`; `pub fn on_mark(&mut self, mark: CommandMark, now: Instant)
    -> Option<Change>`; `pub fn running(&self) -> Option<&str>` (the running command's short form). `Started`
    always starts (replacing one still running) → `Change::Started(short)`; `Ended` with one running →
    `Change::Ended(Finished { short, ending: Ending::of(exit), ran: now − its start })`, else `None`; `Prompt` and
    `Unknown` with one running → `Change::Cleared`, else `None`.
- Produces (`blocks`): `pub fn advance<L: EventListener>(processor, term, tagger, scanner, bytes) ->
  Vec<CommandMark>`: one per mark **applied** (not dropped on the alternate screen or inside a synchronized
  update), in order: `Mark::PromptStart` → `Prompt`; `Mark::OutputStart { cmd, .. }` → `Started { short:
  cmd.as_deref().and_then(short_form) }`; `Mark::Done { exit }` → `Ended { exit }`; `Mark::CommandStart` → nothing.

- [ ] **Step 1: Write the failing tests:**
  - `command_name.rs`: `a_command_runs_from_its_start_to_its_end` (`Started { Some("pnpm dev") }` at `t0` →
    `Some(Change::Started(Some("pnpm dev")))`, `running() == Some("pnpm dev")`; `Ended { Some(0) }` at `t0 + 192 s`
    → `Ended(Finished { short: Some("pnpm dev"), ending: Done, ran: 192 s })`, `running() == None`);
    `an_end_with_no_start_is_nothing`; `a_prompt_ends_the_name_without_a_result` (`Cleared`, then `None`);
    `unknown_forgets_the_running_command`; `a_second_start_replaces_the_first`;
    `a_command_with_no_short_form_still_ends` (`Started { None }` → `Change::Started(None)`, `running() == None`;
    the end's `name()` == `command`).
  - `blocks.rs` (with its `c()` / `d()` helpers): `advance_returns_the_command_marks` (a prompt, `c(Some("API_KEY=
    sk-123 npm run deploy --token abc"))`, output, `d(Some(0))` → `[Prompt, Started { short: Some("npm run
    deploy") }, Ended { exit: Some(0) }]`, collected over the calls); `a_mark_split_over_two_reads_is_returned_once`;
    `marks_on_the_alt_screen_return_nothing` and `marks_inside_a_sync_update_return_nothing` (beside the two
    existing "are ignored" tests); `an_overlong_command_starts_with_no_short_form` (a `C` over `CMD_CAP` → `Started
    { short: None }`).
- [ ] **Step 2: Run** `cargo test --bin fuxx command_name blocks` — FAIL.
- [ ] **Step 3: Implement.** Every caller of `advance` compiles by dropping the result (`let _ =`); behaviour
  unchanged.
- [ ] **Step 4: Run** — PASS; the gate.
- [ ] **Step 5: Commit** `M11q Task 3: command marks out of the parser`.

### Task 4: The marks reach the workspace

**Files:**
- Create: `src/workspace_command.rs` (`impl Workspace`; `mod workspace_command;` in `src/main.rs`),
  `src/ui_shots/command.rs` (new area)
- Modify: `src/terminal.rs` (`TermEvent::Command`, `LocalFeed::batch`, the replica's `Frame::Output` and
  `Frame::Snapshot`, `show_local`), `src/terminal_view.rs` (`TerminalEvent::Command`, field `run: CommandRun`,
  `on_term_event`, `hold`, a ui-shots `shot_output`), `src/workspace.rs` (`Session::command`, the
  `on_terminal_event` arm), `src/workspace_ssh.rs` (`hold`), `src/workspace_daemon.rs` (`daemon_lost`),
  `src/ui_shots/mod.rs`, `src/ui_shots/registry.rs`

**Interfaces:**
- Consumes: Task 3's `CommandMark`, `Change`, `CommandRun`, `advance`.
- Produces:
  - `TermEvent::Command(CommandMark)`. Local: `LocalFeed::batch` sends one per mark on `self.events`, after the
    lock is released and **before** the gate's `Output`, never through `OutputGate`. Replica: `Frame::Output` emits
    them before its `Output`; `Frame::Snapshot` drops what `advance` returns and emits
    `TermEvent::Command(CommandMark::Unknown)` before its `Output`.
  - `TerminalCore::show_local(&self, text: &str) -> Vec<CommandMark>` (callers other than `shot_output` drop it).
  - `TerminalEvent::Command(Change)`: the view's `on_term_event` passes `TermEvent::Command(mark)` to
    `self.run.on_mark(mark, Instant::now())` and emits the change, then `cx.notify()`. `TerminalView::hold` feeds
    `CommandMark::Unknown` the same way.
  - `#[cfg(feature = "ui-shots")] pub fn shot_output(&mut self, text: &str, cx: &mut Context<Self>)`: `show_local`,
    then each returned mark as `on_term_event(TermEvent::Command(mark))` does (the app's path from the bytes on).
  - `Session::command: Option<SharedString>` (the running command's short form; none when nothing runs or it has
    none).
  - `Workspace::command_changed(&mut self, index: usize, change: &Change, cx: &mut Context<Self>)`
    (`workspace_command.rs`): `Started(short)` sets `command`; `Ended(_)` and `Cleared` clear it; a held
    (`disconnected`) session ignores all three; always `cx.notify()` (the chrome parts observe the workspace).
    `Workspace::hold` and `daemon_lost` clear `command`.

- [ ] **Step 1: Write the failing tests** (`terminal.rs`, as its tests run a real `sh` and `wait_for`):
  `command_marks_pass_the_output_gate` (a script printing text, a pause, then `C;cmdline_url=sleep%205` and `D;0`
  marks, with no `output_seen` acknowledged → the channel yields `Command(Started { short: Some("sleep 5") })` and
  `Command(Ended { exit: Some(0) })` though only one plain `Output` was sent);
  `a_snapshot_reports_the_command_unknown` (a replica's `apply(Frame::Snapshot)` whose `vt_bytes` hold a `C` mark →
  the events hold `Command(Unknown)` and no `Started`).
- [ ] **Step 2: Write the failing scene** `command-reaches-workspace` (`command.rs`, State): a plain terminal;
  `shot_output` of a prompt and a `C` mark for `API_KEY=sk-123 npm run deploy --token abc` → the session's
  `command` is `npm run deploy`; a `D;0` mark → `None`; a `C` mark with no `cmdline_url` → `None` while it runs; a
  `C` then a prompt mark → `None`; a `C`, then the pane held (`Workspace::hold`) → `None`, and a `D` after it
  changes nothing; a `C`, then `TerminalEvent::DaemonLost` → `None`. No panic.
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.**
- [ ] **Step 5: Run** — PASS; the gate; `scripts/ui-shots.sh --changed master`.
- [ ] **Step 6: Commit** `M11q Task 4: command marks reach the workspace`.

### Task 5: The end mark settles the status

**Files:**
- Modify: `src/status.rs` (`settles_on_end`, `StatusTracker::on_command_start` / `on_command_end`, module doc),
  `src/terminal_view.rs` (the `TermEvent::Command` arm calls the tracker), `src/workspace_command.rs` (settling),
  `src/ui_shots/command.rs`

**Interfaces:**
- Consumes: Task 4's `Change`, `command_changed`.
- Produces:
  - `pub fn settles_on_end(status: Status, ran: Duration) -> bool` (plan-time call 3).
  - `StatusTracker::on_command_end(&mut self, now: Instant, ran: Duration) -> Option<Status>`: `Some(Done)` when
    `settles_on_end`; always: no attention, nothing new to read, and output is prompt noise until the next keystroke
    (plan-time call 4).
  - `StatusTracker::on_command_start(&mut self, now: Instant) -> Option<Status>`: when a keystroke came since the
    previous start mark, exactly what `on_input(now)` does; otherwise nothing (plan-time call 5).
  - The view, for `Change::Started`: with `tracks_status()`, `on_command_start` and `status_changed` on a change,
    then the `Command` event. For `Change::Ended(fin)`: with `tracks_status()`, `on_command_end(now, fin.ran)`; the
    `Command` event **first**, then `status_changed` if the tracker changed.
  - `command_changed`, on `Ended(fin)`: `if settles_on_end(session.status, fin.ran) { self.set_status(index,
    Status::Done, true, cx) }` (the words come in Task 7).

- [ ] **Step 1: Write the failing tests** (`status.rs`, with its `typed_at` / `ms` helpers):
  - `settles_on_end_table`: Working and Waiting with 0 s → true; Idle with 2 999 ms → false; Idle with 3 s → true;
    Done → false.
  - `an_end_mark_settles_working_at_once` (typed, output → Working; `on_command_end(t, 5 s)` → `Some(Done)` with no
    tick).
  - `the_prompt_after_an_end_mark_is_not_work` (then `on_output` → `None`, `tick` 10 s later → `None`, the status
    stays Done: no second Done).
  - `a_quick_silent_command_stays_idle` (typed; `on_command_end(t, 1 s)` → `None`, Idle; the prompt's output →
    `None`).
  - `a_silent_command_of_three_seconds_is_done` (typed; no output; `on_command_end(t, 3 s)` → `Some(Done)`).
  - `typing_after_an_end_mark_works_as_before` (end → Done; `on_input` → `Some(Idle)`; output after the echo
    window → `Some(Working)`).
  - `a_typed_ahead_command_works_again` (typed, start, output → Working; typed again; end → Done; start →
    `Some(Idle)`; output after the echo window → `Some(Working)`; end → `Some(Done)`).
  - `starts_without_typing_do_not_rearm` (typed, start, output → Working, end → Done; start → `None`; output →
    `None`; end → `None`).
  - `the_silence_guess_still_runs_while_a_command_does` (typed, start, output → Working; quiet 3 s → tick `Done`,
    or `Waiting` with a question on screen; more output → `Working`).
- [ ] **Step 2: Write the failing scene** `command-settles` (State): a terminal that is not the active one; typed
  into, a `C` mark, output until Working, a `D;0` mark → its status is Done on the next frame (no 3 s wait) and the
  row's mark is the Done one; the prompt printed after → still Done, and `notifications` holds one entry. A `C`
  and a `D` with no output and under 3 s → Idle, no entry.
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; `--changed master`.
- [ ] **Step 6: Commit** `M11q Task 5: the end mark settles the status`.

### Task 6: The name (tab, window title, ⌘K, the row's who)

**Files:**
- Modify: `src/groups.rs` (`auto_name`), `src/workspace.rs` (`Session::label`, its doc and `terminal_label`'s),
  `src/daemon.rs` (`notification_summary` passes `None` until Task 10), `src/sidebar.rs` (`WhoFacts::command`,
  `who_of`, `row_who`), `src/ui_shots/chrome.rs` (`chrome-session-facts`), `src/ui_shots/command.rs`

**Interfaces:**
- Consumes: Task 4's `Session::command`.
- Produces:
  - `pub fn auto_name(user: Option<&str>, title: Option<&str>, command: Option<&str>, preset: Option<&str>) ->
    String` (the first not blank, else `DEFAULT_NAME`); `Session::label` passes `self.command.as_deref()`.
  - `WhoFacts { .., pub command: Option<&'a str> }`; `who_of`: an agent (active agent tab, then the agent seen in
    the terminal) first, as today; else `command` as `Who::Program` (`› pnpm dev`); else `program`; else the shell.
    `row_who` passes its terminal's `command`.

- [ ] **Step 1: Write the failing tests:**
  - `groups.rs`: `auto_name_prefers_user_then_title_then_command_then_preset` (`(Some("api"), Some("vim"),
    Some("pnpm dev"), Some("claude"))` → `api`; `(None, Some("vim"), Some("pnpm dev"), ..)` → `vim`; `(None, None,
    Some("pnpm dev"), Some("claude"))` → `pnpm dev`; `(None, None, Some("  "), Some("claude"))` → `claude`; all
    none → `╭∩╮`); the existing test takes the new argument.
  - `sidebar.rs`: `who_reads_the_running_commands_short_form` (`command: Some("pnpm dev")`, `program:
    Some("pnpm")` → `Who::Program("pnpm dev")`, label `› pnpm dev`); `an_agent_in_the_terminal_wins_over_the_command`
    (`terminal_agent: Some("claude")`, `command: Some("claude")` → the Claude agent);
    `an_active_agent_tab_wins_over_the_command`.
- [ ] **Step 2: Write the failing scenes:**
  - In `chrome-session-facts` (M11p: every source of change), three steps on the unnamed session, each
    `Seen::Changes`, driven as its title step is (`emit` of the view's event): "a command starts (OSC 133 C)"
    (`TerminalEvent::Command(Change::Started(Some("pnpm dev".into())))`), "the command ends"
    (`Change::Ended(..)`, ran 1 s, exit 0, with the session looked at), and after a second start "the command is
    cleared" (`Change::Cleared`). After the first: the tab's title, `workspace_label` and the window title last set
    read `pnpm dev`; after each of the others they are back.
  - `command-name` (`command.rs`, `Themes::Both`, Visual): a `fuxx-cli` folder's plain terminal, `shot_output` of a
    `C` mark for `API_KEY=sk-123 npm run deploy --token abc`: the row's line two starts `› npm run deploy`, the tab
    reads `npm run deploy`, the window title `fuxx — #1 npm run deploy`, the ⌘K switcher's row the same; **none of
    the rows' words, the bar's tabs, the tab's tooltip, the window title or the switcher's rows holds `sk-123`,
    `API_KEY`, `--token` or `abc`**. Then: a terminal you named keeps your name on its tab while its row's who still
    reads the command; a terminal whose program set a title (`TitleChanged`) keeps the title on its tab; an agent
    seen in the terminal (`agents_now`) keeps `✳ Claude` as who; the `D` mark puts every name back.
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; `--changed master`.
- [ ] **Step 6: Commit** `M11q Task 6: the pane is named after the running command`.

### Task 7: The banner says which command and how it ended

**Files:**
- Modify: `src/notifier.rs` (`Words`, `words`, `EndTell`, `end_tells`), `src/workspace.rs` (`set_status` split;
  `post_banner`; `Session::done_bannered`, `Session::end_told`; `shot_banners` under `ui-shots`),
  `src/workspace_command.rs` (`end_command`), `src/workspace_agent.rs` and `src/workspace_api.rs` (their banner
  calls go through `post_banner`; words unchanged), `src/ui_shots/command.rs`

**Interfaces:**
- Consumes: Task 2's `Finished`; Task 5's `settles_on_end`.
- Produces (`notifier`):
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Words { pub message: String, pub body: String, pub listed:
    String, pub failed: bool, pub always: bool }` (`message`: what follows `—` in the title; `listed`: the 🔔
    entry's words; `always`: no `QUIET_UNDER`).
  - `pub fn words(status: Status, question: Option<&str>, worked: Option<Duration>, doing: Option<&str>, running:
    Option<&str>, ended: Option<&Finished>) -> Option<Words>`: with `ended`: `message = name()`, `body = words()`,
    `listed = listed()`, `failed = ending.failed()`, `always = true`. Else `body = body(status, question, worked,
    doing)?`, `listed = body`, `message = running` when there is one, else `message(status)?`; `failed` and
    `always` false.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct EndTell { pub settle: bool, pub list: bool, pub banner:
    bool }`; `pub fn end_tells(status: Status, ran: Duration, looking: bool, done_bannered: bool, end_told: bool) ->
    EndTell` (plan-time call 2; `looking` = active session and window focused).
- Produces (`Workspace`):
  - `fn post_banner(&mut self, summary: String, subtitle: String, body: String, about: Option<Terminal>, cx: &mut
    Context<Self>)`: the sound and `notifier::fire` off the UI thread as `set_status` does today; under `ui-shots`
    it only pushes `(summary, subtitle, body)` to `pub(crate) shot_banners: Vec<(String, String, String)>`.
  - `set_status(index, new, notify, cx)` keeps its signature and calls `set_status_told(index, new, notify, None,
    cx)`; `pub(crate) fn set_status_told(&mut self, index: usize, new: Status, notify: bool, ended:
    Option<&Finished>, cx: &mut Context<Self>)` words the notification with `notifier::words(.., running =
    session.command, ended)`; the banner goes out when `words.always || worth_a_banner(new, worked)`; it sets
    `done_bannered` when a banner went out and `end_told` when `ended` was told; both flags are cleared whenever
    the status changes.
  - `end_command(&mut self, index: usize, fin: &Finished, cx: &mut Context<Self>)` (called by `command_changed` on
    `Ended`, replacing Task 5's line): by `end_tells`: `settle` → `set_status_told(index, Done, true, Some(fin),
    cx)`; else `list` → a 🔔 entry (the workspace's name, `fin.listed()`, `Some(Done)`), the `notification` API
    event, and `banner` → `post_banner` with `banner_title(name, fin.name())`, the usual subtitle and `fin.words()`;
    `end_told = true`.

- [ ] **Step 1: Write the failing tests** (`notifier.rs`):
  - `the_banner_table` (the spec's four rows, in today's shape, workspace `fuxx-cli`): `pnpm build`, exit 0, 192 s
    → `banner_title("fuxx-cli", &w.message)` == `fuxx-cli — pnpm build`, body `Done in 3m 12s`; `pnpm dev`, 130,
    3840 s → `fuxx-cli — pnpm dev`, `Stopped after 1h 4m`, `failed` false; `cargo test`, 101, 40 s → `fuxx-cli —
    cargo test`, `Failed (exit 101) after 40s`, `failed` true; `make`, no exit, 120 s → `fuxx-cli — make`,
    `Finished after 2m`. Each `always`; `banner_subtitle(2, None, "fuxx-cli", None)` == `#2`.
  - `a_command_with_no_short_form_banners_command` (`fuxx-cli — command`).
  - `an_agent_started_from_the_prompt_is_named` (`words(Done, None, Some(240 s), None, Some("claude"), None)` →
    message `claude`, body `Worked for 4m`, not `always`; Waiting with a question → message `claude`, body the
    question).
  - `without_a_command_the_words_are_todays` (message `Finished` / `Needs your input`, body as `body()` gives).
  - `end_tells_table`: `(Working, 1 s, not looking, _, _)` → settle, list, banner; `(Working, .., looking)` →
    settle only; `(Waiting, ..)` as Working; `(Idle, 2 s, ..)` → nothing; `(Idle, 3 s, not looking)` → all three;
    `(Done, .., not looking, bannered: false, told: false)` → list and banner, no settle; `(Done, .., bannered:
    true, told: false)` → list only; `(Done, .., told: true)` → nothing; `(Done, .., looking)` → nothing.
  - `a_command_name_is_escaped_like_a_title` (a short form holding `"` and `\`: `applescript_escape` of its title
    has every `"` and `\` escaped and no control character).
- [ ] **Step 2: Write the failing scene** `command-banner` (State; endings driven by the view's `Command` events
  with a chosen `ran`, banners read from `shot_banners`; a "background" terminal is not the active one):
  - `sleep 8 …` ends with exit 0 after 8 s in the background → exactly one banner `("<workspace> — sleep 8 …", "#n…",
    "Done in 8s")`, one 🔔 entry `sleep 8 … · Done in 8s`, one `notification` event with the same title and body.
  - exit 3 after 4 s → `Failed (exit 3) after 4s`; exit 130 on the terminal you are looking at → no banner, no 🔔
    entry.
  - the token line of Task 6 ending in the background → no banner string holds `sk-123`, `API_KEY`, `--token` or
    `abc`.
  - `claude` started (a `C` mark), Working dated 4 min back, then the silence guess's Done (`shot_status`) → one
    banner titled `… — claude`, body `Worked for 4m` (or the terminal's title with `· worked 4m`); then its end
    mark → **no second banner**, and the 🔔 list has one more entry. A second end mark → nothing more.
  - the same with Working dated 5 s back (under `QUIET_UNDER`): the silence Done posts no banner; the end mark
    posts one (plan-time call 2).
  - output, a Done by silence, more output (Working again), the end mark → at most one banner after the end.
  - a terminal with no marks (bash): `StatusChanged(Done)` after Working of 20 s → today's banner, message
    `Finished`.
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; `--changed master`.
- [ ] **Step 6: Commit** `M11q Task 7: the banner names the command`.

### Task 8: The 🔔 list shows a failure

**Files:**
- Modify: `src/notification_list.rs` (`Entry::failed`), every place an `Entry` is built (`src/workspace.rs`,
  `src/workspace_command.rs`, `src/workspace_agent.rs`, `Entry::scripted`, tests, scenes), `src/title_area.rs` (the
  entry's message colour), `src/ui_shots/command.rs`

**Interfaces:**
- Consumes: Task 7's `Words::failed`, `Finished`.
- Produces: `Entry { .., pub failed: bool }` (true only for a command that failed); `title_area`: `pub fn
  message_ink(failed: bool, closed: bool, ui: &UiColors) -> Rgb` (closed → `ui.text_faint`; failed →
  `ensure_contrast(ui.block_failed, <the popover's fill>, TEXT_MIN)`; else `ui.text_dim`), used for the message
  line.

- [ ] **Step 1: Write the failing tests:** `title_area.rs` `a_failed_entry_uses_the_failure_colour` (every theme:
  the failed ink differs from `text_dim`, meets `TEXT_MIN` on the popover's fill; closed wins);
  `notification_list.rs` `a_scripts_notification_is_never_failed`.
- [ ] **Step 2: Write the failing scene** `command-bell-failed` (`Themes::Both`, Visual): two background endings,
  `cargo test` failed (101, 40 s) and `pnpm build` done → the popover's newest entries read `cargo test · Failed
  (exit 101) after 40s` and `pnpm build · Done in 3m 12s` under the workspace's name; a pixel of the first
  message's text is the failure ink, of the second the usual one.
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; `--changed master`.
- [ ] **Step 6: Commit** `M11q Task 8: a failed command in the list`.

### Task 9: Activity gets "command finished"

**Files:**
- Modify: `src/activity.rs` (`Kind::Command`, `ActivityLog::command`, `event_title`, `event_detail`),
  `src/activity_panel.rs` (`RowMark::Command`, `rows_in`, `dot_ink`, its contrast test's list),
  `src/workspace_command.rs` (`end_command` logs), `src/ui_shots/changes.rs` (the `activity` scene)

**Interfaces:**
- Consumes: Task 2's `Finished`, `Ending`.
- Produces:
  - `pub const COMMAND_MIN: Duration = Duration::from_secs(3);`
  - `Kind::Command { name: String, ending: Ending, ran: Duration }` (`name`: the short form or `command`).
  - `ActivityLog::command(&mut self, fin: &Finished, at: SystemTime)`: an entry only when `fin.ran >= COMMAND_MIN`
    or `fin.ending.failed()`.
  - `event_title` → `fin.logged()`'s words (`pnpm build — Done in 3m 12s`); `event_detail` → `None`.
  - `RowMark::Command(Ending)`; `dot_ink`: `Failed` → `ui.block_failed`, `Stopped` → `ui.text_faint`, `Done` and
    `Finished` → `ui.block_ok`; its tooltip is the title (it can be cut).
  - `end_command` calls `session.activity.command(fin, SystemTime::now())` for every ending of a session not held,
    looked at or not, after any status entry.

- [ ] **Step 1: Write the failing tests:** `activity.rs` `a_command_is_logged_when_it_ran_three_seconds_or_failed`
  (2 999 ms Done → none; 3 s Done → one; 0 s `Failed(1)` → one; 1 s `Stopped` → none; 30 s `Stopped` → one);
  `a_commands_words` (`pnpm build — Done in 3m 12s`, no second line); `activity_panel.rs`
  `a_command_row_is_marked_by_its_ending` (rows' marks and tips); the dot contrast test covers
  `RowMark::Command(Failed(1))`, `Command(Stopped)` and `Command(Done)`.
- [ ] **Step 2: Extend the scene** `activity`: on its terminal, a failed ending (`cargo test`, 101, 40 s), a stopped
  one (`pnpm dev`, 130, 30 s) and a quick `ls` (exit 0, 1 s) → two new rows reading `cargo test — Failed (exit 101)
  after 40s` and `pnpm dev — Stopped after 30s`, none for `ls`; the first row's dot pixel is the failure colour,
  the second's the faint one.
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; `--changed master`.
- [ ] **Step 6: Commit** `M11q Task 9: command finished in Activity`.

### Task 10: The background service

**Files:**
- Modify: `src/daemon_session.rs` (`State::run`, `State::ended`, `State::output`, `set_status`,
  `DaemonSession::command`, `take_ended`), `src/daemon.rs` (`notification_summary`, `daemon_banner`, `tick_loop`)

**Interfaces:**
- Consumes: `CommandRun`, `Change`, `Finished`; Task 5's tracker methods; Task 6's `auto_name`.
- Produces:
  - `State::output` passes each mark `advance` returns to `self.run.on_mark(mark, now)`: `Started` →
    `tracker.on_command_start(now)`; `Ended(fin)` → `tracker.on_command_end(now, fin.ran)`, and when that gave
    `Done`, `self.ended = Some(fin)`; each tracker change goes through `set_status` (after the `Output` frame is
    queued, so a window sees the bytes first). `set_status` clears `ended` for any status but Done.
  - `pub fn command(&self) -> Option<String>` (the running short form); `pub fn take_ended(&self) ->
    Option<Finished>`.
  - `fn notification_summary(info: &SessionInfo, command: Option<&str>) -> String` (`auto_name` with the command).
  - `fn daemon_banner(info: &SessionInfo, running: Option<&str>, ended: Option<&Finished>, message: &str) ->
    (String, String)`: with `ended`: (`fuxx — {name()}`, `words()`); else (`notification_summary(info, running)`,
    `message`). `tick_loop` uses it with `session.command()` and `session.take_ended()`; `daemon_should_notify` is
    unchanged (no window attached, a change to Done or Waiting).

- [ ] **Step 1: Write the failing tests:**
  - `daemon_session.rs` (as `status_goes_working_then_settles_once_quiet` runs a real `sh`):
    `an_end_mark_settles_the_service_s_status_at_once` (a script that waits for a typed line, prints a `C` mark for
    `sleep 5`, text, pauses 300 ms and prints `D;0` → after `input`, a `Status` frame `done` arrives well inside
    `IDLE_AFTER`, after the `Output` frame holding the `D`, and no second `done` follows the text printed after it);
    `the_service_knows_the_running_command` (`command()` is `Some("sleep 5")` between the marks, `None` after;
    `take_ended()` gives the ending once).
  - `daemon.rs`: `the_summary_puts_the_command_after_the_title` (the existing summary test with the new argument:
    name → title → command → preset); `the_service_s_banner_names_the_ended_command` (`daemon_banner(info, None,
    Some(&cargo test failed 101 after 40 s), "Finished")` → (`fuxx — cargo test`, `Failed (exit 101) after 40s`);
    no short form → `fuxx — command`; no ending, running `claude` → (`fuxx — claude`, `Finished`); neither →
    today's).
- [ ] **Step 2: Run** `cargo test --bin fuxx daemon` — FAIL. **Step 3: Implement.** No test calls `tick_loop`'s
  post.
- [ ] **Step 4: Run** — PASS; the gate.
- [ ] **Step 5: Commit** `M11q Task 10: the background service settles and names too`.

### Task 11: Hand-off — docs, the full run and the on-screen check

**Files:**
- Modify: `docs/04-Development/68 - Milestone 11q - Which Command Finished.md` ("What was built", "Calls made while
  building" — this plan's plan-time calls and any made in the build —, "Limits", "Final numbers", "On-screen check",
  status), this plan (status table, ticks, "Tasks done"), `docs/04-Development/40 - Roadmap.md` (M11q's line),
  `docs/06-Progress-Log/60 - Progress Log.md` (a new entry), module docs of `terminal.rs`, `status.rs`,
  `blocks.rs` and `groups.rs` where they describe names, marks and settling

- [ ] **Step 1: M11p grep:** `git diff master...HEAD` for added `timer(`, `request_animation_frame`, `spawn(`: none
  expected; any found is listed in note 68 with why it is a one-shot.
- [ ] **Step 2: Leak and public-repo greps** of the branch's added lines: no event or struct field holding a whole
  command line outside `blocks.rs` / `shell_marks.rs`; no `/Users/`, personal names, emails or time-zone names.
- [ ] **Step 3: The full `scripts/ui-shots.sh`** (before the final review); numbers into note 68. After the final
  review's fix wave, the full run again.
- [ ] **Step 4: Write note 68's sections.** Limits to state at least: a wrapper option's value reads as the program
  (`sudo -u deploy make`); one plain word after the program is shown whatever it is (`echo <word>`); several pasted
  lines get one banner (plan-time call 5); the 3 s edge between the window's and the service's clocks (call 3);
  after a restart or reattach the running command is unknown until its next mark; powerlevel10k's own marks carry
  no command (`command`); no marks, no change (bash, fish, `command-blocks = off`). **On-screen check:** the
  spec's 11 items kept, reworded where the banner's shape differs (title `<workspace> — <short form>`, `#n` in the
  subtitle), plus: the 🔔 list's failed entry is red; a command typed ahead gets its own banner; idle CPU with a
  command running is as on master (`top -l 6 -s 5 -pid <pid>`). Run only with `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock
  cargo run` (debug); items 4–7 once with "Keep running after quit" on and once off.
- [ ] **Step 5:** Status everywhere: "🟨 built on branch m11q-command-finished, waiting for your on-screen check;
  not merged". The gate passes.
- [ ] **Step 6: Commit** `M11q Task 11: docs and hand-off` (explicit paths).

## Self-review (done at plan time)

- **Spec coverage.** §1 the name's start and end (T3, T4), the order yours → title → command → preset (T6), where
  it shows: tab, window title, ⌘K, the row's who per the 2026-11-19 update (T6), short form only (T1, T3, T6's
  token probe). §2 the end mark settles at once (T5), the banner table (T2, T7), the focus rule and no minimum (T7
  `end_tells`), the silence guess unchanged and an agent's banners named (T5, T7), one banner per ending (T7), a
  silent command notifies (T5 `a_silent_command_of_three_seconds_is_done`, T7). §3 every rule and table row (T1),
  untrusted text (T1, T7). §4 Activity (T9), the 🔔 list (T7, T8). §5 no new status (nothing adds one; T8 / T9
  colours only). §6 both backends and SSH (T4, T10), no marks no change (T7's bash step), restart unknown (T3, T4),
  no setting (none added). "Would make it a failure": a token in the chrome or Notification Center (T6 and T7
  probes, T11 grep); an agent losing its rings or banners (T5 `the_silence_guess_still_runs…`, T7); two banners
  (T5, T7); a name that lags (T6's three `chrome-session-facts` steps). "Notes for the plan": no protocol change
  (T10), `advance` returns marks and snapshots start nothing (T3, T4), the prompt doesn't re-arm Working (T5), past
  the Output gate (T4), quick silent commands (T5), banner / list / name / Activity sites (T6–T9). On-screen check:
  1–3 (T6), 4–7 (T7; 5–6 also T9), 8 (T1), 9–10 (T5, T7), 11 (T7).
- **Names checked across tasks:** `short_form`, `PROGRAM_CAP`, `WORD_CAP`, `NO_SHORT`, `Ending`, `ran_label`,
  `ending_words`, `Finished` (`name`, `words`, `listed`, `logged`), `CommandMark`, `Change`, `CommandRun`
  (`on_mark`, `running`), `advance`, `TermEvent::Command`, `TerminalEvent::Command`, `shot_output`,
  `Session::command`, `command_changed`, `settles_on_end`, `on_command_start`, `on_command_end`, `auto_name`,
  `WhoFacts::command`, `Words`, `words`, `EndTell`, `end_tells`, `post_banner`, `shot_banners`, `set_status_told`,
  `end_command`, `done_bannered`, `end_told`, `Entry::failed`, `message_ink`, `Kind::Command`, `COMMAND_MIN`,
  `ActivityLog::command`, `RowMark::Command`, `State::run`, `State::ended`, `DaemonSession::command`, `take_ended`,
  `notification_summary`, `daemon_banner`.
- **Proportion:** no function bodies; the short form's reading is given as six ordered rules because its tests
  alone would not fix the order of skipping.
