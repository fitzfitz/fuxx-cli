# Milestone 11u — Restore Where You Were

**Status:** 🟨 built 2026-11-11, released in **v0.7.2** (2026-11-11); waiting for your check.
**Asked for:** 2026-11-11, by you: "can you check did our save session working? my current session setting is on +
agent, but when I reopen it it reset to I don't know when the last saved" → (both causes found) "ya dua-duanya. tapi
bagaimana dengan performance?" → "ya, kerjakan dua-duanya".

## What was wrong

1. **A test build and the app shared one sessions file.** `target/debug/fuxx` (`cargo run`) read and wrote the same
   `sessions.json` as fuxx.app; whichever saved last won, so a reopen could bring back the test build's state.
   **Fixed (a bug fix, with this milestone):** a debug build uses `sessions-dev.json`.
2. **Too little was saved.** Each terminal kept only its name, the folder it *started* in, and the preset it was
   started with. An agent typed at the prompt (`claude`) wasn't known, so "agents" restore had nothing to start, and a
   terminal that had `cd`'d reopened where it started.

## What it does now

- Each terminal's **current folder** is saved (what the shell reported, else what the sidebar last looked up) and a
  reopen starts it there (if the folder still exists; else where it started, else home), in Layout and Agents mode.
- The **agent running in it** is saved when it is one of your presets (matched by the program's name: `claude`,
  `codex`, `opencode`, or an `agent-preset` you added). With `session-restore = agents` a reopen starts it again in
  that folder. Nothing else is ever re-run (a dev server, a build, an editor stay closed).
- The file is written when a folder or an agent changes, at most once a second (no polling added: the folder report
  and the 3 s program check already exist).

## Not in it

Re-running arbitrary commands, restoring scrollback, SSH sessions' remote folder (they reconnect as before), the
background service's sessions (they are live anyway).

## As built (2026-11-11)

- `restore.rs`: `state_path` → `state_path_for(home, cfg!(debug_assertions))` (`sessions-dev.json` for a debug build);
  `SavedSession::{cwd, agent}` (left out of the file when none, so old files load and stay as they were); `launch`
  takes the saved preset, else the preset named by `agent`, from today's presets (Agents mode only).
- `presets.rs`: `agent_for(program, presets)`: the preset named so, or whose command's first word it is.
- `workspace.rs`: `folders_now` (from a `FolderMoved` report on this Mac) and `agents_now` (`note_agents`, after each
  3 s program check that changed something); `saved_state` writes them (a folder the sidebar looked up when the shell
  never reported one; nothing for an SSH session); `save_soon` writes at most once a second; `start_in` starts in
  `cwd` when it still exists.
- Tests: 1357 pass (new: the dev file, an agent typed at the prompt restored by Agents only and never a non-preset,
  the fields round-trip and stay out of old files, a reopened terminal starts where it was or where it started).

## On-screen checklist (for you)

1. `cd` somewhere, quit, reopen: the terminal is in that folder.
2. Type `claude` in a terminal, quit, reopen (Agents): claude starts again there.
3. `npm run dev` (or any non-agent) is not restarted; the terminal is at its prompt in that folder.
4. Running `cargo run` (a test build) no longer changes what fuxx.app reopens.
