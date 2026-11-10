---
tags: [development, milestone, design]
status: designed
---

# 68 - Milestone 11q - Which Command Finished (design)

**Status:: 📝 Designed 2026-11-10, approved by you the same day ("cool update this to the plan docs so we can work on
it later"). Plan not written yet (paused for the sidebar redesign; see "Notes for the plan"). Not built.** Comes **after M11p** (your call,
2026-11-10: "ok, brainstorm it, after M11p"). You skipped the design questions ("you go ahead"), so every decision
below except the three in "Decided by you" is **the AI's call**: check those while reading.
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
3. Then **one plain word** (letters, digits, `-_.:`, at most 24 characters, not starting with `-`), the subcommand.
4. If that word is `run`, `exec`, `x` or `dlx`, **one more plain word** (the script's name: `npm run deploy`).
5. If the line goes on to another command (`&&`, `||`, `;`, `|`), add ` …`.
6. A line that can't be read this way (starts with `$(`, `(`, a backtick or an unclosed quote) has no short form:
   the pane keeps its usual name and the banner says `command`.

| Command line | Short form |
|---|---|
| `pnpm dev` | `pnpm dev` |
| `cargo test --release -- --nocapture` | `cargo test` |
| `API_KEY=sk-123 npm run deploy --token abc` | `npm run deploy` |
| `sudo -E ./scripts/build.sh prod` | `build.sh prod` |
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

## Left to the builder

- How the marks reach the workspace (a new `TerminalEvent`, for the local and the background-service backends).
- The exact colours (from the existing failure and muted tokens).
- How the duration is worded past an hour (reuse `age_label`'s style).

## On-screen check (when built)

1. In a plain zsh pane, run `sleep 5`: the row, tab and window title read `sleep 5` while it runs, then go back.
2. Name the pane yourself, run `sleep 5`: your name stays.
3. Run `vim`: the pane shows vim's own title, not `vim`.
4. Start `sleep 8; true` in one pane, switch to another: a banner `#n <folder> — sleep 8 …` · `Done in 8s`.
5. Run `sh -c 'sleep 4; exit 3'`, switch away: `Failed (exit 3) after 4s`; Activity shows it red.
6. Run `sleep 30`, press ⌃C: no banner (you were looking); Activity shows `Stopped`.
7. Run `FOO=secret sleep 6 --x` in the background: banner and row read `sleep 6`, never `FOO`, `secret` or `--x`.
8. Run `npm run <script>` (any project): the name is `npm run <script>`.
9. Start `claude` from the prompt, let it work and finish in the background: the ring works as before; the banner
   reads `— claude` · `Worked for …`, once.
10. A command that prints, pauses 5 s, then ends: no more than one banner after it ends.
11. In bash (`exec bash`), run a command: everything as before (no name, the old banner).
