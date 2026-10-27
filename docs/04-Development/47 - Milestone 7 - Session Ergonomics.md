---
tags: [development, milestone]
status: in-progress
---

# 47 - Milestone 7 - Session Ergonomics

**Status:: 🟨 In progress (plan written 2026-10-27).**  ·  **Phase 2: living in fuxx all day.**
**Plan:** [[M7 Plan - Session Ergonomics]] (step-by-step, with status per task)

## Goal

Sessions you can organise and get back. Sessions sit in **project groups** (one per folder) with a
rolled-up ring. You can **rename** and **reorder** them, start **claude / opencode / codex** (or your own
agents) in one action, and after quitting, fuxx **reopens the same sessions**. ([[40 - Roadmap]] M7;
[[11 - Features]] B.)

## Your choices (2026-10-27)

- Build M7 now, before your M6b on-screen check. All the 👀 checks are done together at the end.
- **Agent presets:** the three built-ins (claude, opencode, codex) **plus your own** from the settings file.
- **Session restore:** bring back the layout **and re-run the agent presets**, and make it **a setting**
  you can change.

Everything else below is the AI's call. Say if any should change.

## How it behaves

### Project groups

- **File → New Session in Folder… (⌘O)** opens the macOS folder picker. The new session's shell starts
  in that folder, and the session joins that folder's **group** (the group is created if needed).
- **⌘T** opens a new session in the **same group as the session you're in**. If that session has no
  group, the new one has none either and starts in your home folder, as today.
- **Sessions without a group** sit at the top of the sidebar with no header.
- **Group header:**
  - an arrow (▾ open / ▸ closed), then the folder's name. If two groups' folders have the same name,
    the parent folder is added, e.g. `api (work)`.
  - the group's **most urgent ring**: waiting (red) beats done (green), done beats working (yellow),
    and working beats idle.
  - a **+** button that opens a new session in that group.
  - Clicking the header opens or closes the group.
- A group's sessions are always kept together, and a new one goes at the end of its group. When a
  group's last session closes, the group disappears.
- **Closed groups:** their sessions keep running and still count for ⌘1…⌘9, which follow the sidebar
  order. Switching to a hidden session (⌘N, ⌘⇧[ / ⌘⇧]) opens its group.

### Renaming

- **Double-click** a session in the sidebar, or **File → Rename Session… (⌘R)** for the current one.
  The name turns into a text box that starts with the current name.
- Typing adds to the end. Backspace deletes, and ⌘Backspace clears it. **Enter** keeps the new
  name, and **Escape** or clicking elsewhere cancels.
- Emoji and accented letters work, through the macOS emoji picker (⌃⌘Space) and ⌥ keys.
- An empty name goes back to the automatic one.
- **Which name shows:** your name, else the program's own title, else the agent preset's name, else 🖕.
  The window title and notifications use the same name.

### Reordering

- **Drag** a session up or down in the sidebar and drop it on another session to take its place.
  This works only inside the same group, or among sessions without a group. A drop anywhere else is
  ignored.
- **File → Move Session Up / Move Session Down** do the same from the menu (no shortcut).

### Agent presets

- **File → New Agent Session ▸** lists **claude**, **opencode**, **codex**, then your own, with
  shortcuts **⌃⌘1…⌃⌘9** in that order.
- A preset opens a new session **in the current session's group** (like ⌘T). The shell starts
  normally, then the preset's command is **typed into it and Enter pressed**. When the agent exits, you're
  back at the shell prompt in the same session.
- **Your own presets** go in `~/.config/fuxx/config`, one line each:
  - `agent-preset = aider: aider --model sonnet` adds "aider".
  - The same name as a built-in replaces it, e.g. `agent-preset = claude: claude --continue`.
  - A name with no command removes it, e.g. `agent-preset = codex:`.
  - `agent-preset =` (empty) resets the list to the three built-ins.
  - At most 9 presets; extra ones are skipped with a log line, as are lines with no `:` or no name.
- Like the other settings, presets are read **at startup**. Restart fuxx after editing them.

### Session restore

- **The setting:** `session-restore = agents | layout | off` in the settings file. The default is
  `agents`.
  - `agents`: reopen every session, and re-run each agent preset.
  - `layout`: reopen every session as a plain shell.
  - `off`: always start with one new session, as today.
- The **Preferences window** gets a **Restore sessions** row with three buttons: **Off**, **Layout**
  and **Layout + agents**. It's saved like the other settings and applies from the next launch.
- **What comes back:** the sessions in the same order with your names, their groups and folders, which
  groups were closed, and the session you were in.
- **What doesn't:**
  - the screen text and scrollback;
  - programs other than presets;
  - a folder you `cd`'d into later. A session restarts in its group's folder, or your home folder.
- **When it's saved:** to `~/Library/Application Support/fuxx/sessions.json`, written safely (temp
  file + rename) whenever you open, close, rename, move or switch sessions, or open/close a group.
  Quitting (⌘Q or the window's red button) keeps the list as it was.
- **Closing the last session** quits fuxx, and saves an empty list. So the next launch starts with one
  new session.
- **If something's wrong:**
  - If the file is missing, unreadable or from an unknown version, fuxx starts one new session and
    logs a line.
  - If a group's folder no longer exists, its sessions start in your home folder and keep the group's
    name.

## Rough scope / checklist

- [ ] Project groups: New Session in Folder (⌘O), ⌘T joins the current group, headers with rolled-up ring, open/close, +.
- [ ] Rename: double-click or ⌘R, Enter/Escape, emoji input, empty = automatic name.
- [ ] Reorder: drag within a group; Move Session Up/Down.
- [ ] Agent presets: built-in three + `agent-preset` lines, ⌃⌘1…9, command typed into the shell.
- [ ] Session restore: `session-restore` setting + Preferences row; saved on every change; restored at launch.

## Not in M7

- Per-project settings, nested groups, or detecting projects from `cd`.
- Dragging a session into a different group, or reordering whole groups.
- Editing presets in the Preferences window.
- Restoring screen text, scrollback, or any running program other than presets.
- Picking up hand edits to the settings file without a restart.

## Success check

Open two folders with ⌘O and start claude in one with ⌃⌘1. Rename a session to "🖕 api", drag it
above its neighbour and close the other group. Quit with ⌘Q and reopen: the same groups, names, order,
closed group and current session come back, and claude starts again on its own.

## Related

- [[40 - Roadmap]] · [[11 - Features]] · [[12 - Scope]] · [[46b - Milestone 6b - Settings Window]] · [[00 - Plan Tracker (Start Here)]]
