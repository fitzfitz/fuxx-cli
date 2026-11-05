---
tags: [development, milestone]
status: planned
---

# 51 - Milestone 10b - SSH Workspaces

**Status:: ⬜ Planned (spec written 2026-11-05).**  ·  **Phase 3: agents on other machines, in the same sidebar.**
**Plan:** [[M10b Plan - SSH Workspaces]] · **Evidence:** [[Input - M10 Feasibility Study]] §3

## Goal

Open a session on another machine as easily as a local one. Pick a host, and fuxx opens a pane running your own
`ssh` to it. A `claude` running there shows the same status rings and notifications as a local one. A second pane
to the same host opens instantly. When the network drops, the pane says so and one key reconnects.

## Your choices (2026-11-05)

- You said "m10b go", and to skip the questions. **Every choice below is the AI's call**, taken from the
  feasibility study (Option A). Say if any should change.

## How it behaves

### Opening an SSH session

- **File → New SSH Session… (⇧⌘S)** opens a host picker in the window:
  - hosts from `~/.ssh/config` (following `Include`, skipping wildcard patterns like `Host *`);
  - hosts you used recently, first;
  - a box to type any host (`user@host`, `host`, or an alias).
  - Hosts that are only for git hosting are hidden (`User git`, or GitHub, GitLab, Bitbucket and Azure DevOps
    addresses), so the list shows machines you can log in to.
- One option in the picker: **Keep running on the host (tmux)**. When on, the remote side runs inside
  `tmux new -A -s fuxx-<name>`, so the remote programs survive network drops and you can reattach. It needs tmux on
  the host; without tmux, the pane shows the remote error.
- From scripts: `fuxx new --ssh HOST [--tmux]` (and the same in `fuxx mcp`).
- **⌘T or a split while an SSH session is active** opens another session to the same host, with the same
  option, so a second pane to a host is one key away.
- The pane runs your Mac's own `ssh` with a terminal (`ssh -t`), so your keys, agent, `~/.ssh/config`, jump hosts
  and 2FA all work as in any terminal. Password, 2FA and new-host-key prompts appear in the pane.
- The host is passed to `ssh` as one argument, never through a shell. A host starting with `-` is refused.
- Unlike an agent preset (typed into your login shell), the pane runs `ssh` itself, so fuxx can tell a dropped
  connection from a normal logout.
- If `ssh` can't connect at all (unknown host, refused, wrong key), the pane stays open with ssh's error and the
  same "press Enter to reconnect" line, instead of vanishing.

### In the sidebar

- SSH sessions are grouped by host (like project groups in M7), labelled with the host name, with the group's
  rolled-up ring.
- Status detection is unchanged: `ssh` passes the remote bytes through, so a remote agent's bell, notification
  codes and screen text drive the rings and notifications exactly like a local one.

### Connection reuse and keepalive

- A second session to the same host reuses the first connection (one login), unless your `~/.ssh/config` already
  sets `ControlMaster` for that host, in which case yours is used.
- fuxx keeps the shared connection open for 10 minutes after the last pane to that host closes.
- Keepalive: fuxx asks `ssh` to check the link every 20 s, unless your config sets it, so a dead link is noticed
  in about a minute instead of hanging.

### When the connection drops

- `ssh` ends with its "connection failed" code (255). The pane **keeps its screen** and shows one line:
  "Disconnected from HOST — press Enter to reconnect". The ring shows done, and the usual done notification
  follows the M4 rules.
- **Enter reconnects** in the same pane (same place, name and group), with the same host and options. The new
  connection starts on a fresh screen.
- With the tmux option on, fuxx reconnects by itself, with a growing wait (3, 6, 12 … up to 60 s), because the
  remote programs are still there to come back to. Without tmux, a reconnect is a new remote shell, so fuxx waits
  for your Enter. Automatic reconnecting happens while the window is open; with the window closed (M10a), a dropped
  SSH session ends and comes back through restore when you reopen.
- When the remote shell ends normally (you type `exit`), the session ends as any session does today.

### With other milestones

- **M7 restore:** SSH sessions are saved with their host and tmux option. With `session-restore = layout` or
  `agents`, a cold restore runs `ssh` again (connecting is the place, not a program): with tmux that's a true
  reattach, without it a fresh remote shell. With `off`, they don't come back.
- **M10a persistent sessions:** the local `ssh` lives in the background service like any session, so an SSH session
  survives quitting fuxx with no extra work. Only network drops and the Mac sleeping end it.
- **M9 scripting:** `fuxx` commands on your Mac reach SSH sessions like any other (`send`, `read`, `wait`,
  `answer`). On the remote host, `fuxx` and `$FUXX_SESSION` are not available.

### Limits to know

- **No remote control of fuxx.** Commands run on the remote host can't drive fuxx. Forwarding fuxx's socket would
  let anyone with access to that host type into your local sessions, so it isn't done. A remote fuxx service is a
  later step.
- **tmux and notifications.** Inside remote tmux, notification codes reach fuxx only if tmux is set to pass them
  through (`set -g allow-passthrough on`, tmux 3.2+). The screen-text and bell rules still work. The tmux setup is
  yours.
- **A stuck shared connection** can hold up new panes to that host. fuxx checks the shared connection before reusing
  it and starts a fresh one if it's dead.
- Nothing is installed on the remote host.

## Rough scope / checklist

- [ ] Host list from `~/.ssh/config` (Include, no wildcards, git-only hosts hidden) + recent hosts + typed host.
- [ ] Host picker in the window; "New SSH Session…" in the File menu.
- [ ] `ssh` command built from argv only, with connection reuse and keepalive unless configured; leading `-`
  refused.
- [ ] Sidebar groups by host.
- [ ] Disconnect line + Enter to reconnect; automatic reconnect with tmux.
- [ ] Saved and restored (M7), works in the background service (M10a).
- [ ] `fuxx new --ssh HOST [--tmux]` and the MCP tool.

## Not in M10b

- A fuxx service on the remote host (persistent remote sessions without tmux, remote `fuxx` / MCP). That's
  "Option B" in the study, a later milestone.
- Picking a remote folder to start in (use tmux or your remote shell's own setup).
- mosh, port forwarding UI, a remote file browser.
- Forwarding `FUXX_*` variables or fuxx's socket to the host.

## Success check

1. File → New SSH Session… → pick a host. A second pane to the same host opens instantly, with no second login.
2. Run `claude` on the host. When it asks something, the ring goes to waiting and a notification arrives.
3. Turn Wi-Fi off. Within about a minute the pane shows "Disconnected from HOST — press Enter to reconnect". Turn
   Wi-Fi on, press Enter: connected again.
4. With the tmux option: drop the network, and fuxx reconnects by itself to the same remote programs.
5. With persistence on (M10a): quit fuxx and reopen. The SSH session is still connected.

## Related

- [[40 - Roadmap]] · [[Input - M10 Feasibility Study]] · [[50 - Milestone 10a - Persistent Sessions]] ·
  [[47 - Milestone 7 - Session Ergonomics]] · [[49 - Milestone 9 - Scripting API]]
