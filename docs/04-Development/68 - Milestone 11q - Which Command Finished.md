---
tags: [development, milestone, design]
status: built-on-branch
---

# 68 - Milestone 11q - Which Command Finished

**Status:: 🟨 built, **merged to master and pushed 2026-11-20** (`f93654b`), waiting for your on-screen check.** Designed 2026-11-10 and approved by you the same day; re-read with you 2026-11-19 ("go ahead"; see "Update 2026-11-19"); planned and built 2026-11-19 in 12 tasks (0–11), plan [[M11q Plan - Which Command Finished]]. Comes **after M11p** (your call,
2026-11-10: "ok, brainstorm it, after M11p"). You skipped the design questions ("you go ahead"), so every decision
below except the three in "Decided by you" is **the AI's call**: check those while reading. The whole-branch review is done and its fix wave applied (see "Calls made while building"); a second full ui-shots run and a full run on your usual display are still owed before merging (see "Final numbers").
**Phase 4: an M11 extension (the chores round after v0.5.0).** Note 67 is left for M11p's design.

## Goal

When a command started at the shell prompt is running, the pane is **named after it** (`pnpm dev`, `cargo test`).
When it ends, the notification, the 🔔 list and Activity say **which command it was and how it ended**
(`pnpm build` · Done in 3m 12s; `cargo test` · Failed (exit 101) after 40s), without ever showing the rest of the
command line.

## Why (your words, 2026-11-10)

- "npm/cargo/pnpm/dan lain-lain untuk jalankan proses dev, test, build. Saya mau titlenya berubah juga… jika saya
  selesai jalankan dan dapat notifikasi, saya tidak tahu itu apa"
- "ok, brainstorm it, after M11p" · "you go ahead"

## What exists today

- **The banner** (`workspace.rs` `set_status`, `notifier::body`): title `#<n> <workspace> — Finished`, body
  `Worked for 3s`. The 🔔 list keeps the workspace's name and the same body. Nothing names the command.
- **The name** (`groups::auto_name`): yours → the program's own title (OSC 0/2) → the preset's name → `╭∩╮`. Your
  zsh (powerlevel10k, no title set) leaves plain shell panes as `╭∩╮` in the sidebar, the tab and the window title.
- **"Done" is a guess from silence** (`status.rs`): output makes a session Working; 3 s of quiet (`IDLE_AFTER`)
  makes it Done, or Waiting with a question on screen; OSC 9 / 99 / 777 and BEL settle it sooner.
- **The shell already says when a command starts and ends.** fuxx's zsh integration (M11l, `shell_integration.rs`)
  sends OSC 133 `C;cmdline_url=<command>` as a command starts and `D;<exit>` as it ends; `shell_marks.rs` decodes
  both and `blocks.rs` uses them for the command blocks (`Outcome`: Ok / Failed / Unknown / Running). No
  `TerminalEvent` carries them to the workspace, so names and notifications can't use them yet.
- `agent_detect::program` (M11k) already finds a command's program after leading `NAME=value` words.
- `activity.rs` (M11h) logs status changes, notes and progress per terminal (in memory, last 200).

## Decided by you

1. **The pane's name changes to the running command** ("Saya mau titlenya berubah juga").
2. **A finished command's notification says which command it was** ("saya tidak tahu itu apa").
3. **After M11p.**

## AI's calls (you skipped the questions)

### 1. The running command's name

- A command's name **starts with its `C` mark and ends with its `D` mark** (or the next prompt mark, or the shell
  exiting, whichever comes first).
- The name order becomes: **yours → the program's own title → the running command → the preset's name → `╭∩╮`**.
  Your name always wins; a program that titles itself (vim, Claude Code) keeps its title, which says more.
- It shows everywhere the name shows today: the sidebar row, the pane tab, the window title, ⌘K.
- Only the **short form** (§3), never the whole command line.

### 2. "Finished" means the end mark

- A command's `D` mark **is** its end: the session settles to Done at once (no 3 s wait), and a banner names it.
- What the banner says:

  | How it ended | Banner title | Banner body |
  |---|---|---|
  | exit 0 | `#2 fuxx-cli — pnpm build` | `Done in 3m 12s` |
  | exit 130 (⌃C) | `#2 fuxx-cli — pnpm dev` | `Stopped after 1h 4m` (not a failure) |
  | any other non-zero exit | `#2 fuxx-cli — cargo test` | `Failed (exit 101) after 40s` |
  | no exit code sent | `#2 fuxx-cli — make` | `Finished after 2m` |

- **The same focus rule as today**: no banner for the terminal you're looking at (active session, window focused).
  No minimum length: a quick command you typed is one you were looking at.
- **The silence guess stays while a command runs**, unchanged. An agent started from the prompt (`claude`, `codex`)
  keeps going Working / Waiting / Done exactly as today, and its banners now carry its name:
  `#2 fuxx-cli — claude` · `Worked for 4m`, or the question it asks.
- **One banner per ending.** If the silence guess already posted Done for this command and nothing was printed
  since, the end mark adds its result to Activity and the 🔔 list but posts no second banner.
- A command that prints nothing (`sleep 60; git push -q`) never looked Working, so today it never notified; with
  the end mark it does. That is intended.

### 3. The short form (what fuxx shows of a command line)

Command lines can hold tokens, and Notification Center keeps its banners on disk, so fuxx shows only a short form,
built from the start of the line:

1. Skip leading `NAME=value` words, and the wrappers `sudo`, `env`, `time`, `nohup`, `command`, `exec` with their
   own options (words starting with `-`) and `NAME=value` words.
2. The program: its last path part (`./scripts/build.sh` → `build.sh`).
3. Then **one plain word** (letters, digits, `-_.:`, at most 24 characters, not starting with `-`), the subcommand —
   **only when the program is one known to take subcommands** (`git`, `npm`, `pnpm`, `yarn`, `cargo`, `go`, `make`,
   `docker`, `kubectl`, … — `SUBCOMMANDS` in `src/command_name.rs`). Any other program shows its name alone, because
   its next word may be a value, even a secret (`echo sk-…`). A leading `cd <folder> &&` (or `;`) is skipped and the
   command after it is named. (Your call, 2026-11-20: "go with 1, fix the cd one too".)
4. If that word is `run`, `exec`, `x` or `dlx`, **one more plain word** (the script's name: `npm run deploy`).
5. If the line goes on to another command (`&&`, `||`, `;`, `|`), add ` …`.
6. A line that can't be read this way (starts with `$(`, `(`, a backtick or an unclosed quote) has no short form:
   the pane keeps its usual name and the banner says `command`.

| Command line | Short form |
|---|---|
| `pnpm dev` | `pnpm dev` |
| `cargo test --release -- --nocapture` | `cargo test` |
| `API_KEY=sk-123 npm run deploy --token abc` | `npm run deploy` |
| `sudo -E ./scripts/build.sh prod` | `build.sh` |
| `echo sk-live-123` | `echo` |
| `cd web && pnpm dev` | `pnpm dev` |
| `cargo build && cargo test` | `cargo build …` |
| `curl -H "Authorization: Bearer x" https://…` | `curl` |
| `$(which node) server.js` | (none) |

The short form is the **only** form fuxx shows outside the terminal itself: in banners, the 🔔 list, the sidebar,
tabs, the window title, Activity and the scripting API's events. The full line stays where it is today, in the
terminal and the block's Copy command. Like an OSC title, a mark can be printed by any program's output (`cat` of a
file), so its text is untrusted: control characters dropped, length capped, escaped for AppleScript as titles are.

### 4. Activity and the 🔔 list

- **Activity** gets a "command finished" entry: `pnpm build — Done in 3m 12s`, marked like its block's gutter bar
  (red for Failed, grey for Stopped). Only commands that ran **3 s or more, or failed**, so a shell's `ls` and `cd`
  don't push older entries out of the 200.
- **The 🔔 list** entry carries the same words as the banner (`pnpm build · Failed (exit 101) after 40s`); a failed
  one uses the failure colour.

### 5. How a failure looks

No new status. A failed command settles as Done like any other; the failure is in the words (banner, 🔔 list,
Activity) and in the block's red gutter bar, which exists already. A separate Failed status would reach the rings,
the cursor colour and the scripting API's status names, a bigger change than this chore.

### 6. Where it works

- Any terminal whose shell sends OSC 133 `C` with `cmdline_url` and `D`: fuxx's zsh integration, locally or in the
  background service, and an SSH host whose shell sends them itself.
- **Without marks nothing changes**: bash and fish (not injected), `command-blocks = off`, programs started as a
  preset's command without a shell prompt.
- After fuxx restarts while a session runs in the background service, the running command is unknown until its next
  mark; the pane shows its usual name until then.
- No setting of its own: `command-blocks = off` turns the integration off, and this with it.

## Not in it

- Changing the silence guess. A quiet `pnpm dev` can still look Done while it runs, as today.
- Shell integration for bash or fish.
- A Failed status, a red ring or a red cursor.
- Showing more of a command line anywhere.
- New scripting API events or fields (the existing events carry the new names and words).

## Would make it a failure even if it works

- A token from a command line ending up in Notification Center, the sidebar or a screenshot of the chrome.
- An agent started from the prompt losing its Working / Waiting / Done rings or banners.
- Two banners for one ending.
- A name that lags: the sidebar, tab and window title must change when the command starts and go back when it ends
  (M11p's rule: chrome always shows current information). Each source of change gets a test.

## M11p comes first, and this builds on it

M11p splits the chrome into cached views that redraw only when their own data changes. The running command's name
is a new source of change for the sidebar row, the tab and the window title, so it must notify those views; its
tests sit with M11p's "every source of change gets a test".

## Notes for the plan (found 2026-11-10, plan not written yet)

You approved this design on 2026-11-10 ("cool update this to the plan docs so we can work on it later"). Writing the
plan was paused when you turned to the sidebar redesign. What the code reading found, for whoever writes it:

- **No protocol change needed.** A background-service session's window replica already runs `blocks::advance` on
  the same output bytes (`terminal.rs` ~1124), so the window can read the C/D marks itself. Only the service's own
  `StatusTracker` (`daemon_session.rs` `State::output`) needs the end mark too, to settle Done, plus its own
  banners when no window is attached (`daemon.rs` `daemon_should_notify` / `notification_summary`; `SessionInfo`
  can take extra serde-default fields).
- **Where the marks are:** `blocks::advance` (`blocks.rs:431`) loops `scanner.feed(bytes)` and drops marks on the
  alternate screen or inside a synchronized update. Make it return the marks it applied; callers: `terminal.rs`
  (local parser ~307, `show_local` ~757, snapshot ~1096, replica output ~1124), `daemon_session.rs:486`,
  `vt_snapshot.rs` (×2), tests. Snapshot replays should not start a command name.
- **Settling must stop the prompt re-arming Working.** `StatusTracker::on_output` turns Done back into Working on
  any output after a keystroke, so the prompt drawn after `D` would make a second "Worked for 0s" banner. The end
  mark should make output before the next keystroke count as prompt noise (as `last_input = None` does at startup).
  Command events must not go through M11n's one-wake-per-batch Output gate.
- **Quick silent commands** (`cd`, `export`): settling Idle → Done on every one would pop the row each time. Settle
  to Done only from Working/Waiting, or from Idle when the command ran ≥ 3 s (`IDLE_AFTER`); this is what §2's
  "silent commands now notify" needs.
- **Banner:** `workspace.rs` `set_status` builds it from `notifier::message(new).zip(notifier::body(..))` and
  `notification_summary(number, who, message)` = `#<n> <who> — <message>`; the command's short form replaces
  `message`. The 🔔 list's `notification_list::Entry` has no failure flag yet; the failure colour is `ui.block_failed`
  (`block_view.rs`).
- **Name:** `groups::auto_name(user, title, preset)` is called from `workspace.rs` `Session::label` and `daemon.rs`
  `notification_summary`; the running command goes between title and preset. `TerminalEvent` (`terminal_view.rs:81`)
  gets a command variant, handled next to `TitleChanged` (`workspace.rs` ~1925).
- **Activity:** `activity::Kind` (+ `activity_panel.rs` `RowMark`, and the matches in `workspace_ssh.rs:698`,
  `ui_shots.rs`) gets a command-finished kind.
- Reusable: `agent_detect::program` (leading `NAME=value` words), `status::age_label` (duration style).

## Update 2026-11-19 (before the plan)

You reviewed the summary of this design on 2026-11-19 and said "go ahead". Since the design was written:

- **M11p is on master** (merged; its branch is gone), so this builds on it as planned.
- **The sidebar row now says who · where** (M11al, Task 8b: who follows the workspace's active tab). The row no
  longer shows the pane's name, so §1's "the sidebar row" means the row's **who** (AI's call):
  - When the active tab is a terminal with a running command that has a short form, who reads `› <short form>`
    (`› pnpm dev`) instead of the bare program (`› pnpm`).
  - An agent fuxx detects in that terminal still wins (`✳ Claude`), as does an agent tab.
  - A name you gave, or a program's own title, changes the tab and the window title as in §1; the row's who is not
    a name, so those don't apply to it.
- The tab, the window title and ⌘K follow §1 unchanged.
- The line numbers in "Notes for the plan" are from 2026-11-10; the plan re-finds them.
- The Activity panel keeps its timeline look (M11al); the "command finished" entry is one more event on it.

## Left to the builder

- How the marks reach the workspace (a new `TerminalEvent`, for the local and the background-service backends).
- The exact colours (from the existing failure and muted tokens).
- How the duration is worded past an hour (reuse `age_label`'s style).

## What was built (2026-11-19, branch `m11q-command-finished`)

- **The pane is named after the command running at its shell prompt.** The order is yours → the program's own title →
  the running command's short form → the preset's name → `╭∩╮`. It shows in the tab, the window title and ⌘K; the
  sidebar row's **who** reads `› <short form>` (an agent fuxx detects still wins). The name starts at the `C` mark and
  ends at the `D` mark, a prompt mark, a reattach snapshot, an SSH hold or a lost service connection. (Tasks 3, 4, 6.)
- **The short form** (`src/command_name.rs`, Task 1) is the program, plus the subcommand when the program takes subcommands (`SUBCOMMANDS`) and the next word is a plain
  word (a leading `cd <folder> &&` skipped, 2026-11-20), plus ` …` when more follows; env assignments, flags, paths and quoted text never appear. Nothing but the
  short form leaves the terminal: `blocks::advance` returns the marks it applied as `CommandMark`s already reduced to
  it, so no event or struct field holds a whole command line (the full line stays in the grid's tag and
  `shell_marks.rs`).
- **The end mark settles the status at once** (Task 5): Working or Waiting → Done; Idle → Done only after a run of 3 s
  or more; Done stays. The prompt that follows does not re-arm Working. A start mark re-arms Working only after a
  keystroke since the previous start (typed ahead works, pasted lines do not add banners).
- **The banner says which command and how** (Task 7): title `<workspace> — <short form>`, subtitle `#n` (plus the folder group when it differs from the workspace's name), body `Done in
  3m 12s` / `Stopped after 30s` / `Failed (exit 101) after 40s` / `Finished after …`. One banner per ending, decided by
  the pure `notifier::end_tells`; the silence guess (3 s, `IDLE_AFTER`) is unchanged, so is the banner minimum (10 s, `QUIET_UNDER`), and an agent's banners name the command too.
  Every banner goes through one `Workspace::post_banner`, which under `ui-shots` records the strings instead of posting.
- **The 🔔 list** (Task 8) shows `<name> · <words>`; a failed entry is drawn in `ui.block_failed`.
- **Activity** (Task 9) logs `<name> — <words>` for a command that ran 3 s or more or failed; a failure is red.
- **The background service** (Task 10) settles at the end mark with its own tracker and, with no window attached, posts
  its own banner `fuxx — <short form>` · the ending words. No protocol change (`SessionInfo` is unchanged); its command
  state is read from the session, not sent.
- **Commits** (from plan commit `ab18273`): `1ced124` Task 0 baselines · `37e48ae` Task 1 · `1959e65` Task 2 ·
  `2f3eb68` Task 3 · `57584ef` + `bb95ee5` Task 4 (+ fix) · `3e7f728` Task 5 · `7d07a01` Task 6 · `1309729` Task 7 ·
  `bfd03bd` Task 8 · `b58cda1` Task 9 · `2dc33fb` + `3dd6fed` Task 10 (+ fix) · Task 11 this hand-off.

## Calls made while building

**Plan-time calls (AI's, 2026-11-19; the full text is in the plan):**

1. The banner keeps today's shape (title `<workspace> — <short form>`, subtitle `#n`, plus the folder group when it differs from the workspace's name), not the design's 2026-11-10 layout.
2. No minimum length for an end-mark banner; the silence guess keeps its 3 s and the banner minimum its 10 s. "One banner per ending" is kept by two per-session flags.
3. The Workspace settles at the end mark for both backends by the same pure rule the trackers use; the window and the service time the command with their own clocks.
4. After an end mark, output is prompt noise until the next keystroke.
5. A start mark re-arms Working only after a keystroke since the previous start (several pasted lines get one banner).
6. Short-form details the design left open (the word right after the program; ASCII letters and digits; a 32-character cap; no short form for `$`, `(`, a backtick or an unclosed quote; `&&`, `||`, `;`, `|` and newlines count outside quotes only).
7. Durations use the two largest units (`40s`, `3m 12s`, `1h 4m`, `1d 2h`).
8. An unnamed workspace's row follows `Session::label`, so it reads the short form too.
9. The name ends at the end mark, a prompt mark, a snapshot, an SSH hold and a lost service connection; a snapshot never starts one.
10. The service's own banner and how it reads its command state (no protocol change).
11. Tests post nothing: one `Workspace::post_banner`, recording under `ui-shots`.
12. The exact copy (`Done in {d}`, `Stopped after {d}`, `Failed (exit {n}) after {d}`, `Finished after {d}`, `command`).

**Made in the build (AI's):**

- **P1:** an item whose first non-test caller came in a later task carried a temporary `#[allow(dead_code)] // until M11q
  Task N`; all were removed by Task 10 (a grep finds none).
- **P2:** banners use call 1's shape, not the design table's 2026-11-10 layout.
- **E1 (8 scenes fail on this machine's display):** after the build was paused and resumed, 8 pixel/layout scenes fail
  on the machine's display of that moment (one external non-Retina screen; half-pixel checks such as "818 vs 818.5"):
  card-split, chrome-pixels-same, file-code-bars, file-tab, file-tab-md-reader-column, right-panel-comfortable,
  tab-bars, title-area. They fail identically on untouched Task 3 code built from `git archive`, and the same code had
  0 failures before the pause, so they are held to be environmental, not this branch's. Every reference fingerprint
  also reads "looks different" there.
- **F4a:** Task 4's scene went through `hold` and posted a real banner ("api — Finished"); fixed in the scene
  (`bb95ee5`); the guard for every path is Task 7's `post_banner`.
- **N1:** from Task 5 on every test and ui-shots run had a recording stand-in `osascript` first on PATH. It showed that
  the scene `chrome-notifications` (already on master) posted two real banners per ui-shots run ("agent — Finished",
  "api — Finished"); since Task 7 nothing posts.
- The view handles a mark in one helper, `TerminalView::command_marked`; for an end it emits the `Command` event first
  and the tracker's status change after (pinned by scene `command-banner`).
- `shot_banners` became one plain `Vec` behind `post_banner`; a `ui-shots` build run as the app posts no banners. A
  ui-shots-only probe `Workspace::shot_events` records `emit`'s lines for scenes.
- `Workspace::hold` clears the running command for any session, before its ssh check.
- The `activity` scene's "no waiting dot" probe was narrowed: the failed dot's colour (`block_failed`) is within the
  probe's tolerance of the waiting ring's colour.
- Service: `ended` is kept only when the end mark itself gave Done and is cleared by any other status.

**The final review and its fix wave (2026-11-20):**

- The whole-branch review found no Critical issue and called the branch ready with fixes: two Important (the order of a batch's command marks before its output was not pinned by a test; this note's on-screen check had items that would read as failures though the code is right), and minors.
- Fixed in the wave: I-1 (two ordering tests, local and background-service replica), I-2 (this note's on-screen items), and M-1 to M-10: the service takes an ending only for a Done; an unnamed workspace's silence banner reads `pnpm dev — Finished`; an ending's subtitle names no program; `chrome-notifications` re-runs when the ending code changes; a scene step for forged marks after a told ending; a few moved or stale comments, `outputs <= 1`, more pinned wording rows, and a fuxx-cli-own example string.
- Stays deferred (the review's triage): the typed-ahead start that swallows an ending in the service (needs a queue of endings instead of a polled status; only with no window attached); the short-form shapes that fail safe (escaped quote, `&& "ls"`, `sleep 5&`, bidi); a notification in the same batch as an end mark; scene strength notes other than M-8; real-`sh` timing in the service's tests (300 ms gaps; revisit only if it flakes); small trivia (a `_now`, closures, a shadow, `feed_marks`, a `debug_assert`).
- Ruling E1 was re-checked by the controller with a full run on untouched `ab18273`. E1 re-check (2026-11-20): the full suite on untouched `ab18273`, exported to a scratch copy with its own build folder, gave 239 passed and 8 failed: the same 8 scenes, failing at the same checks as on this branch (`chrome-pixels-same` differs by 358 in both; `tab-bars` reads 818 against 818.5). So the 8 are the display's, not the branch's. The re-review of the fix wave then found doc wording still wrong (plan-time call 1's subtitle, on-screen items 16 and 18, two Limits lines, counts in the Roadmap and the Progress Log); those were corrected in the same docs commit that filled these numbers, by the controller, and that commit had no review of its own. One comment in the `command-banner` scene still says "three terminals" (there are four).

## M11p check (the chrome rule)

Greps over `git diff master...HEAD` (2026-11-19, Task 11):

- No `timer(` and no `request_animation_frame` added.
- Added `spawn(`: three are test code, two in `daemon_session.rs` tests (the helper that runs a `sh -c` script as a
  session, in `an_end_mark_settles_the_service_s_status_at_once` and `the_service_knows_the_running_command`) and
  `TerminalCore::spawn` in a `terminal.rs` test; they start a shell or thread for the length of one test. One
  production hit: `cx.background_spawn(... notifier::fire ...)` inside `Workspace::post_banner`, which is the old
  one-shot banner post moved into one place: it runs once per banner and has no loop.
- No struct field or event holds a whole command line outside `src/blocks.rs` and `src/shell_marks.rs` (the added
  `line` hits are `&[u8]` parameters of a scene helper).
- No home-directory paths except the existing generic fixture path (`me/code/fuxx-cli`), whose visibility changed; no
  personal names, emails or time-zone names.
- The running command's start, end and clearing each have a test (the three steps of scene `chrome-session-facts`,
  Task 6).

## Limits

- A wrapper option's value reads as the program: `sudo -u deploy make` shows `deploy make` (the design's rule).
- A program not in `SUBCOMMANDS` loses its subcommand too (`./build.sh prod` → `build.sh`, `sleep 5` → `sleep`); add a
  program to the list to show its subcommands.
- Several pasted lines get one banner (plan-time call 5), the first line's; the later lines run with the pane reading
  Done and end silently (Activity still logs those that ran 3 s or more, or failed). A bracketed paste is one command, so this is the unbracketed case.
- Two commands typed ahead while one runs: the second is not re-armed (no Working for it).
- With no window attached, an ending after a silence-guess "Finished" banner is not told at all (the service has no list).
- There is no minimum length for an ending's banner, and that covers script-driven panes too: a script sending short
  commands into a background pane gets a banner and a sound for each one that prints 150 ms or more after it starts
  (output sooner counts as the echo of what was typed, and then a command under 3 s tells nothing); master posted
  none under 10 s.
- An interactive program run for 3 s or more (vim, less, man, a REPL) leaves its row Done, with the pop, when you quit it.
- After an end mark, output is noise until a keystroke, so a backgrounded job (`pnpm dev &`) never shows Working or
  Done (master's silence guess did).
- Exits 146 and 148 (⌃Z) and 143 read "Failed", in red.
- An old background service left running across an upgrade gives one extra 🔔 entry per silent command, and no extra banner.
- The window and the service time a command with their own clocks, so a command within a few ms of 3 s from Idle can
  settle in one and not the other (a `Finished` banner, or a Done the next status frame corrects): the 3 s edge.
- After a restart or a reattach the running command is unknown until its next mark.
- powerlevel10k's own marks carry no command text, so such a pane keeps its usual name and the banner says `command`.
- No marks, no change: bash, fish and `command-blocks = off` behave as before (no name, the old banner).
- Short form: a backslash-escaped quote inside double quotes (`git commit -m "fix \"x\""`) reads as an unclosed
  quote, so no short form (`command`); `sleep 5&` (glued `&`) gives `sleep`; a remainder made only of a quoted word
  (`make && "ls"`) gets no ` …`; only control characters are dropped from a program's name, not bidi or format characters.
- A notification (OSC 9 / BEL) that arrives in the same output batch as the end mark can raise attention again after
  the ending's Done (Waiting if the tail looks like a question).
- Service with no window attached: a typed-ahead command's start can clear the previous ending before the 50 ms tick
  sees it, so that ending gets no banner.
- The real posting path (sound, `notifier::fire`, native and osascript) and the service's own banner are compiled out
  of, or not reached by, tests: they are on-screen check items.
- **Deferred minors** (the final review's triage; the full per-task list is in the build ledger): a few scene steps
  assert weaker than they could (`assert_ne!` where equality is known; a hold step that does not reach the workspace
  filter); the Task-7 scene lacks steps for a looked-at Done followed by an end after looking away, and a background
  exit 130; `command-bell-failed` copies the popover's private layout constants; `shot_output` does not end with
  `cx.notify()`.

## Final numbers

Measured 2026-11-19 by the Task 11 hand-off on branch `m11q-command-finished` at `3dd6fed` (plus docs):

- `cargo test --bin fuxx`: 2206 passed, 0 failed, 1 ignored (baseline 2154). `cargo clippy --all-targets`: 14 (bin) / 27 (test) warnings; with `--features ui-shots`: 16 / 29 (all unchanged from the baselines). `cargo fmt --check`: clean.
- Full `scripts/ui-shots.sh` (no filter, recording stand-in `osascript` first on PATH): **244 passed, 8 failed, 115.3 s** in the script's own count (about 123 s wall including the build check). The 8 that failed are exactly ruling E1's: card-split, chrome-pixels-same, file-code-bars, file-tab, file-tab-md-reader-column, right-panel-comfortable, tab-bars, title-area; no other scene failed. The recording stand-in's `posted.log` stayed 597 bytes (no test or scene tried to post a banner).
- Baselines at the plan commit: 2154 tests passed, 1 ignored; clippy 14 / 27 default, 16 / 29 `ui-shots`.
- **The 8 failing scenes are held to be environmental** (ruling E1 above): the same 8 fail on untouched Task 3 code and
  the same code had 0 failures before the pause. **A full run on your usual display is still owed before merging.**
- After the final review's fix wave (2026-11-20, at `1eaeb50`): `cargo test --bin fuxx` 2208 passed, 0 failed, 1 ignored (two ordering tests added); clippy 14 / 27 and 16 / 29, unchanged; `cargo fmt --check` clean (the fix wave's own gate). Full `scripts/ui-shots.sh` again, stand-in first on PATH: **244 passed, 8 failed, 137.9 s**: the same 8 scenes, at the same checks as the untouched plan commit (see the E1 re-check above); no banner was posted.

## On-screen check

Run the app only as `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug, not `--release`). Items 4–7 once with
"Keep running after quit" on and once with it off. The banner's shape is the plan's: title `<workspace> — <short form>`,
subtitle `#n`, plus the folder group when it differs from the workspace's name; body the ending words.

1. In a plain zsh pane, run `sleep 5`: the tab and window title read `sleep` while it runs (the row's who reads
   `› sleep`), then go back. Run `git status` the same way: `git status`.
2. Name the pane yourself, run `sleep 5`: your name stays.
3. Run a program that sets its own title (stock vim sets none unless `:set title`, so it would read `vim`): the pane keeps the program's title.
4. Start `sleep 8; true` in one pane, switch to another: a banner titled `<workspace> — sleep …`, subtitle `#n` (plus the folder group when it differs from the workspace's name),
   body `Done in 8s`.
5. Run `sh -c 'sleep 4; exit 3'`, switch away: `Failed (exit 3) after 4s`; Activity shows it red.
6. Run `sleep 30`, press ⌃C after 3 s or more: no banner (you were looking); Activity shows `Stopped` (it logs a stop only from 3 s).
7. Run `FOO=secret sleep 6 --x` in the background: banner and row read `sleep`, never `FOO`, `secret`, `6` or `--x`.
8. Run `npm run <script>` (any project): the name is `npm run <script>`.
9. Start `claude` from the prompt, let it work and finish in the background: the ring works as before; the banner
   reads `<workspace> — claude`, once; its body is `Worked for …`, or `<the agent's title> · worked 4m` when the agent sets a title (an unnamed workspace's title part is then that title too).
10. A command that prints, pauses 5 s, then ends: no more than one banner after it ends.
11. In bash (`exec bash`), run a command: everything as before (no name, the old banner).
12. The 🔔 list's failed entry (from item 5) is drawn in red.
13. A command typed ahead while another runs gets its own banner when it ends.
14. Idle CPU with a command running is as on master: `top -l 6 -s 5 -pid <pid>` while `sleep 120` runs, against the same on master.
15. Quit the window with "Keep running after quit" on while `sleep 20; true` runs: one banner, `fuxx — sleep …` ·
    `Done in 20s`, from the service.
16. An unnamed plain zsh pane running a program that sets no title of its own (one that sets a title reads as in
    item 9), or a long silent-then-chatty command, in the background: the silence
    banner's title reads `… — Finished` (or `… — Needs your input`), not the command twice; say if you want it changed.
17. The failed dot in Activity against the waiting ring's colour: are they distinct enough to you?
18. A script loop sending quick printing commands into a background pane (for example `fuxx send` of `sleep 1; echo hi` a few
    times; a bare `echo hi` prints too soon to count as work and tells nothing): expect a banner and a sound per command (plan-time call 2 in action). Say if that is too noisy.
19. Run `sleep 5 && echo hi &` in a pane: expect no Working and no Done for the background job (plan-time call 4).
20. Run `echo sk-test-123` in a background pane: the tab, row and any banner read `echo`, never the word after it.
21. Run `cd src && sleep 4` in a background pane: the pane reads `sleep`, not `cd src …`, and its banner names `sleep`.

## Chores after M11q (2026-11-20, layout)

Your ask: "let's do layout chores then just go straight to master", with screenshots of what to change. Built on
master, test first; full gate green (2215 tests; clippy 14/27 and 16/29; fmt); full ui-shots 243 passed, the failing
ones the known display-dependent scenes (checked against master in a worktree: same failures; `tab-bars` now passes).

1. **Pane tabs are small pills** ("I want the tab like this, a pill, not that big and the + is instead beside the
   last tab"; "the buttons look too big and the tab x padding is too big"; "the gap between tab and edges is too big").
   Tabs 28 pt tall with round ends, as wide as their titles (measured, 100–220 pt), 8 pt side padding, 4 pt clear
   between pills, no separator line; every tab a quiet `card_hover` pill, the shown one the accent wash (an unfocused
   pane's shown one `card_lift`). **+ follows the last tab**; ⋯ and the right sidebar's stay pinned right. Bar buttons
   24 pt (14 pt icons). The bar, the window's top band and the right panel's strip are **40 pt** (52 before), tabs
   6 pt in from the edge; the traffic lights stay centred (y 14).
2. **An icon set** ("find icon with outline no backgrounds… we need to have icon sets"): Lucide's outline icons
   (lucide-static 0.460.0, ISC, `assets/icons/lucide/`), embedded and served as GPUI's asset source (`src/icons.rs`);
   the screenshot harness gets them too. Tab icons are now outline icons with no tile behind them; the bar's +, ⋯
   and right-sidebar buttons use the set. **AI's call:** Lucide (you can name another set).
3. **Files / Changes / Activity switch: icons only, full width, tooltips** ("a full width tab with icons only,
   tooltip for the label"): folder, git-compare and activity icons in pill segments as tall as a tab, 6 pt from the
   panel's sides; the shown one `card_lift`, the others light on hover; each segment's name in its tooltip.
4. **Collapsed sidebar: its button beside the traffic lights** ("on collapse, put the sidebar toggle button beside
   the 3 window button on the left"): at x 80 (13 in full screen); the 🔔 and ⚙ stay at the right; the centred name
   keeps clear of both. **AI's call:** the button waits until a hide slide ends (the sliding sidebar carries its own).

**On-screen check (yours):** with `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`:
1. Tabs are pills as wide as their titles, the + right after the last one; long titles stop at 220 pt with `…`.
2. Many tabs shrink to 100 pt, then the row scrolls; + stays in sight after them.
3. Tab icons are thin outlines with no square behind them; ⋯ and the panel button look the same weight.
4. The right panel's switch shows three icons; hovering each says Files / Changes / Activity.
5. ⌘\ to hide the sidebar: its button appears just right of the traffic lights; clicking it brings the sidebar back.
6. Dragging a tab still shows the gap and drops where it shows.

## Chores after M11q, part 2 (2026-11-20, the critique's five fixes)

A UI/UX critique of the pill-tab chores (impeccable, design review only: native app) scored the chrome 24/36 and
named five issues; your call: "fix all five". Mid-way, your screenshot of the collapsed band: "fix alignment and add
more acceptable gaps". Built on master, test first.

1. **One icon set everywhere.** Every hand-drawn line icon (the bars', the sidebar's search, bell, gear, folders and
   chevrons, the Changes bar's, the file and browser bars', the callouts', the update pill's) is now drawn from Lucide
   (`tab_bar::Glyph` is `icons::Icon`; 41 files in `assets/icons/lucide/`). The sidebar button is Lucide's
   panel-left. The per-icon stroke widths went (the set has its own).
2. **No coloured tiles in the sidebar.** A row's avatar keeps its letters on a hairline outline with no fill; the
   letters are dim, or your tab colour when you set one (`avatar_ink`). The name-picked gradient palette is gone from
   the themes. Colour in a row now means status. **AI's call:** keep the letters (they tell rows apart).
3. **One line across the window's top.** The traffic lights, the sidebar band's title and button, and every pane
   bar's pills are centred on `CHROME_LINE` (28 pt: the 8 pt strip plus half the 40 pt bar). The sidebar's band is
   48 pt (down to the bars' bottom). On macOS 26 the lights are 14 pt, 22 apart; the button after them starts 12 pt
   past the green one (your "more acceptable gaps"; it was about 8).
4. **One right-sidebar button per window**, on the top-right pane's bar; the other panes keep only ⋯ (and +).
5. **Collapsed: no second header.** With the sidebar hidden there is no band over the card: the card rises to the
   8 pt strip, and [sidebar] [🔔] [⚙] sit by the traffic lights in the top-left pane's bar, whose tabs start 12 pt
   after the ⚙ (that room drags the window). The 🔔's popover drops from the 🔔. The workspace's name is no longer
   shown there (the sidebar's row says it). **AI's call:** the 🔔 and ⚙ go left with the sidebar button, not right,
   so they never sit over the right panel's switch.

**Checks:** 2215 tests; clippy: no new warnings; fmt; full ui-shots 246 passed, 6 failed, all six the known
display-dependent scenes (`title-area`, one of them before, passes now).

**On-screen check (yours):** with `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`:
1. The traffic lights, "the Fuxx?", the sidebar button and the tab pills share one line.
2. ⌘\ hides the sidebar: no band appears over the terminals; [sidebar] [🔔] [⚙] sit just right of the lights with
   a clear gap, the tabs after them; pressing the empty space after the ⚙ drags the window.
3. The 🔔 there opens its list just under it; the ⚙ opens Preferences; the sidebar button brings the sidebar back.
4. Split right and down: only the top-right pane has the right-panel button; ⋯ is in every pane.
5. Sidebar rows show letters on an outline (no coloured squares); a row you coloured shows its letters in that colour.
6. Icons everywhere (search, bell, gear, folders, chevrons, Changes bar, file bar) look like one family.

## Chores after M11q, part 3 (2026-11-20, each theme's own chrome colours)

Your ask: "rework the color combination of each theme… it doesn't match good with the combination"; then "just
dive in, give each theme its own chrome colours". Built on master, test first.

**Before:** every theme file holds only the terminal's colours, and one shared recipe derived all the chrome from
them: the accent was always palette 4 (blue), working 3 (yellow), needs you 1 (red), done 2 (green), and the
canvas the background darkened 42 %. Gruvbox got Nord's treatment.

**Now:** a theme can set five chrome colours, `chrome-canvas`, `chrome-accent`, `chrome-working`,
`chrome-waiting` and `chrome-done` (in a theme file or your settings; empty clears one); each replaces what the
recipe would derive, still held to the same contrast floors. The built-in themes carry them in fuxx's own
`assets/themes/chrome/` files (the upstream files untouched; `NOTICE.md` says so), all from each theme's published
palette:

| Theme | Canvas | Accent | Working | Needs you | Done |
|---|---|---|---|---|---|
| Catppuccin Mocha | crust `#11111b` | mauve `#cba6f7` | peach `#fab387` | red `#f38ba8` | green `#a6e3a1` |
| Catppuccin Latte | crust `#dce0e8` | mauve `#8839ef` | peach `#fe640b` | red `#d20f39` | green `#40a02b` |
| Dracula | `#191a21` | purple `#bd93f9` | cyan `#8be9fd` | orange `#ffb86c` | green `#50fa7b` |
| Nord | `#242933` | frost `#88c0d0` | yellow `#ebcb8b` | red `#bf616a` | green `#a3be8c` |
| Gruvbox Dark | bg0_h `#1d2021` | orange `#fe8019` | blue `#83a598` | purple `#d3869b` | green `#b8bb26` |
| TokyoNight | `#15161e` | blue `#7aa2f7` | yellow `#e0af68` | red `#f7768e` | green `#9ece6a` |

**AI's calls:** one rule for every theme: neutrals carry the structure, the accent alone means "you are here",
and each status colour stands at least 25° of hue from the accent (a test holds it). Gruvbox's working is blue,
not yellow (15° from its orange); Dracula's needs you is orange, not pink (it read like the purple selection, seen
in the six-theme renders). The status marks and tab colours are now also held to 3:1 on the selections (Latte's
mauve made them dip to 2.93); Mocha's dim and faint text moved a step, Latte's three darker, to stay readable.

**Checks:** 2218 tests (one daemon start-up test failed once in a full run and passed three times alone and in the
next full run); clippy: no new warnings; fmt; full ui-shots 246 passed, 6 failed (the known display-dependent
scenes).

**On-screen check (yours):** Settings → Appearance, each built-in theme in turn:
1. The selected row, the shown tab and the left edge bar are the theme's accent (Mocha/Latte mauve, Dracula purple,
   Nord frost, Gruvbox orange, TokyoNight blue).
2. A row that needs you is clearly a different colour from the selected one.
3. The sidebar sits darker than the terminal card (lighter-grey on Latte) and the two read as one family.
