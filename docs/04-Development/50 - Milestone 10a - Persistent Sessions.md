---
tags: [development, milestone]
status: in-progress
---

# 50 - Milestone 10a - Persistent Sessions

**Status:: 🟨 In progress (plan written 2026-10-27).**  ·  **Phase 3: agents keep working when fuxx doesn't.**
**Plan:** [[M10a Plan - Persistent Sessions]] · **Evidence:** [[Input - M10 Feasibility Study]] §2

## Goal

When you turn it on, your sessions **keep running after you quit fuxx**, or after it crashes. Reopening fuxx
brings every session back **live**: the same shells and agents, their screens and scrollback, their status
rings. Agents can keep working, asking and finishing while the window is closed, and fuxx still notifies you.

## Your choices (2026-10-27)

- You said "m10 go".
- **Opt-in:** off by default. Quitting ends sessions, as today, unless you turn this on.
- **Notifications** keep coming while the window is closed.
- **Scripting** with the window closed (`fuxx` / `fuxx mcp`) comes later, not in M10a.
- The rest is the AI's call, based on the feasibility study. Say if any should change.

## How it behaves

### Turning it on

- In the settings file, `persistent-sessions = on`. Or use **Preferences → Sessions → Keep running after quit**.
  Off by default.
- The setting takes effect at the next launch. Sessions that are already open stay as they are.

### When it's on

- fuxx starts a small **background service** (`fuxx daemon`, the same program). The service owns your sessions.
  The window shows them and sends your typing to them. You see no difference in speed, because the window still
  draws the screen itself.
- **Quitting** (⌘Q or the red button) leaves your sessions running.
  - The menu says **Quit fuxx (N sessions keep running)**.
  - It also has **Quit fuxx and End All Sessions (⌥⌘Q)**.
- **Closing a session** (⌘W, or the program exits) still ends it, as today.
- **Reopening fuxx** brings every running session back:
  - with the screen and scrollback as they are now;
  - in the same groups, layouts, names and order;
  - with its current status. An agent that started waiting while you were away shows the waiting ring.
  - Sessions you had that are no longer running (for example after a restart of the Mac) come back through session
    restore (M7), as today.
- **While the window is closed:**
  - status keeps being tracked;
  - **notifications still arrive** when an agent needs you or finishes;
  - programs that ask the terminal questions (cursor position, colours, size) still get answers, so nothing hangs.
- **If fuxx crashes**, the sessions survive, just as when you quit.
- **Session ids stay the same** while the background service runs (`s4` is still `s4` after you reopen fuxx). So
  `fuxx` commands typed in a surviving shell still reach the right session.
- **The background service ends by itself** about a minute after the last session closes and no window is open.
  It never lingers with nothing to do.
- `fuxx daemon status` shows whether it's running and how many sessions it holds.
- `fuxx daemon stop` ends the service. It refuses while sessions are running unless you add `--force`, which ends
  them.

### When it's off (the default)

Exactly as today: no background service, and quitting ends every session.

### Limits to know

- **Logging out or restarting the Mac** ends the background service and its sessions, as it does for tmux.
  Session restore (M7) brings the layout back on the next launch.
- **macOS privacy folders.** A process that outlives fuxx might be blocked from Documents, Desktop or Downloads.
  This is checked first (Task 0). If it's blocked, the setting's description says so plainly until signing (M12).
- **Same-user access.** The background service holds every session's screen and accepts typing for them while the
  window is closed. Only your macOS user can reach it, through a private socket like M9's, and nothing listens on the
  network.
- **Scripting needs the window** for now. `fuxx …` commands and `fuxx mcp` reach the open window, as in M9.

## Rough scope / checklist

- [ ] Task 0 check: a background process outliving fuxx can still use your Documents and Desktop (or the limit is documented).
- [ ] `persistent-sessions` setting + Preferences row; off by default.
- [ ] Background service owns sessions; the window reattaches with screen, scrollback, status; ids stable.
- [ ] Quit keeps sessions; "Quit and End All Sessions" (⌥⌘Q); ⌘W still ends one.
- [ ] Reopen places live sessions in their groups and layouts; dead ones go through M7 restore.
- [ ] Notifications and terminal answers while the window is closed.
- [ ] `fuxx daemon status` / `stop [--force]`; the service ends itself when idle.

## Not in M10a

- Scripting (`fuxx`, `fuxx mcp`) while the window is closed. That's a later step.
- Sessions surviving logout or reboot.
- Upgrading fuxx without ending running sessions. A new fuxx works with the old service if they're compatible;
  otherwise it explains and offers to restart the service.
- Several windows, or remote (SSH) background services (M10b covers SSH differently).

## Success check

With persistence on:
1. Start `claude` in two sessions, one of them in a split, and quit with ⌘Q.
2. While fuxx is closed, one agent asks a question: a notification arrives.
3. Reopen: both sessions are back with their full scrollback, the layout is intact, the asking one shows the
   waiting ring, and you can answer it.
4. Repeat after killing fuxx with `kill -9` instead of ⌘Q: same result.

## Related

- [[40 - Roadmap]] · [[Input - M10 Feasibility Study]] · [[47 - Milestone 7 - Session Ergonomics]] · [[49 - Milestone 9 - Scripting API]]
