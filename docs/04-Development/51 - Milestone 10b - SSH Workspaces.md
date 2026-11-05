---
tags: [development, milestone]
status: built
---

# 51 - Milestone 10b - SSH Workspaces

**Status:: 🟨 Built and reviewed (2026-11-06); on-screen check pending.**  ·  **Phase 3: agents on other machines, in the same sidebar.**
**Plan:** [[M10b Plan - SSH Workspaces]] · **Evidence:** [[Input - M10 Feasibility Study]] §3

## Goal

Open a session on another machine as easily as a local one. Pick a host, and fuxx opens a pane running your own
`ssh` to it. A `claude` running there shows the same status rings and notifications as a local one. A second pane
to the same host opens instantly. When the network drops, the pane says so and one key reconnects.

## What shipped

Built on branch `m10b-ssh-workspaces` (plan: [[M10b Plan - SSH Workspaces]]), every task reviewed, plus a whole-branch
review and a fix round. Not yet run on screen: you will check M10b, M10c and M11 together (see below).

- **Host picker** (⇧⌘S, File → New SSH Session…): config hosts, recent hosts first, a typed host, a tmux tick box.
- **Panes run your own `ssh`**, grouped by host in the sidebar, reusing one connection per host, with keepalive.
- **Disconnect line and reconnect:** the pane keeps its screen; Enter reconnects; with tmux it reconnects by itself.
- **Saved and restored** (M7) and kept alive by the background service (M10a, which moved to protocol 2).
- **`fuxx new --ssh HOST [--tmux]`**, the MCP option, and `fuxx ls` showing the host.
- 457 tests pass; no new crates.

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
  `tmux new -A -s fuxx-<host>-<random>` (each pane gets its own name, see Decisions), so the remote programs survive network drops and you can reattach. It needs tmux on
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

## Rough scope / checklist (all built; on-screen check pending)

- [x] Host list from `~/.ssh/config` (Include, no wildcards, git-only hosts hidden) + recent hosts + typed host.
- [x] Host picker in the window; "New SSH Session…" in the File menu.
- [x] `ssh` command built from argv only, with connection reuse and keepalive unless configured; leading `-`
  refused.
- [x] Sidebar groups by host.
- [x] Disconnect line + Enter to reconnect; automatic reconnect with tmux.
- [x] Saved and restored (M7), works in the background service (M10a).
- [x] `fuxx new --ssh HOST [--tmux]` and the MCP tool.

## Not in M10b

- A fuxx service on the remote host (persistent remote sessions without tmux, remote `fuxx` / MCP). That's
  "Option B" in the study, a later milestone.
- Picking a remote folder to start in (use tmux or your remote shell's own setup).
- mosh, port forwarding UI, a remote file browser.
- Forwarding `FUXX_*` variables or fuxx's socket to the host.

## Decisions made along the way

Rulings taken during the build and review (AI's calls, in keeping with the plan). The ones you would notice:

- **Each tmux pane gets its own tmux session**, named `fuxx-<host>-<6 random letters/digits>`, made when the pane
  opens, saved with it, and reused when it reconnects or is restored. Before, two panes to one host shared a tmux
  session and mirrored each other.
- **A pane is held (not closed) when ssh exits 255, or exits with any non-zero code in under 10 seconds.** An early
  failure (unknown host, no tmux on the host) keeps ssh's error on screen with
  `ssh ended (exit N) — press Enter to reconnect`. Those non-255 holds never reconnect by themselves. Other exits
  close the pane as before.
- **SSH panes start from your login shell's environment.** An app opened from Finder has no `.zshrc` settings, so
  fuxx captures them once (`$SHELL -ilc`, 3 second limit, kept for the run). Your agent socket, Homebrew
  ProxyCommand tools and the like then work as in a terminal. If your shell startup is slow, the first SSH open can
  take up to 3 seconds longer.
- **A reconnect starts on a fresh screen.** The line is written after a reset (normal colours, normal screen), so a
  drop while `vim` or `tmux` was running does not leave the line garbled.
- **One notification per drop run.** Failed retries in a row stay quiet. A connection counts as having been up only
  if it showed output and lasted a minute; only then do retries start again from 3 seconds and a new drop notify.
- **Auto-reconnect no longer steals focus** from an open picker or rename box, or from the pane you are typing in.
- **Restore checks saved hosts.** A saved session with a bad host (for example one starting with `-`) is not
  started, and the sidebar says why.
- **`fuxx ls` shows `[ssh:HOST]`** after the name; `fuxx ls --json` has an `"ssh"` field for SSH sessions only.
- **`fuxx new --ssh` cannot be combined with `--folder`, `--preset` or `--run`**, and `--tmux` needs `--ssh`.
- **A background service from before M10b must be restarted** once (`fuxx daemon stop`); until then an SSH session
  shows "restart the background service to use SSH sessions".
- A hung shared connection is cleared: if the health check times out, that host's own socket is removed so the next
  pane starts a fresh connection.
- A `# comment` after a `Host` line in `~/.ssh/config` is ignored (it used to add junk rows).
- A script's `send` to a disconnected pane is refused ("disconnected — reconnect first"); `read` still works.

## Known limitations

- No remote fuxx service (remote `fuxx`/MCP, persistent remote sessions without tmux): a later milestone.
- Notification codes inside remote tmux need `set -g allow-passthrough on` (yours to set).
- Automatic reconnect runs only while the window is open.
- An explicit `ControlMaster no` in your ssh config is treated like the default, so fuxx's reuse options are added.
- Typing a host that is a substring of a recent one selects the recent row first (Enter opens it); "Connect to …"
  is the last row.
- A remote shell that itself exits 255 (for example `ssh other` failing, then `exit`) reads as a disconnect.
- A tmux pane closed with ⌘W leaves its remote tmux session behind; list them with `tmux ls` on the host.
- A reconnect attempt shows a blank pane until ssh says something (up to about 75 s on a black-holed network).
- `session.list` does not say a held pane is disconnected; scripts find out when input is refused.
- The picker's checks (`ssh -G`, `-O check`) use fuxx's own environment, so from Finder they may use a different
  `ssh` than the pane does.
- Smaller items are listed in the plan's Deferred minors.

## On-screen checklist (pending)

Run these together with the M10c and M11 checks. Needs a host you can log in to (ideally one that needs a Homebrew
ProxyCommand or a shell-exported `SSH_AUTH_SOCK`) and, for tmux, a host with tmux.

1. Run fuxx from the .app opened in Finder (not from a terminal) for all checks below, unless one says otherwise.
2. File menu order: New Session, New Session in Folder…, New SSH Session… ⇧⌘S (shortcut shown).
3. ⇧⌘S opens a centred panel with the dim placeholder "Host — type or pick"; typing goes into the box, not the terminal.
4. Rows: recent hosts first (with a dim "recent" or "recent · tmux" tag), then your config hosts; no wildcard hosts;
   git hosts disappear within about a second. A `Host work # office` line shows only `work`.
5. Typing `PROD` (different case) narrows the list and adds "Connect to PROD" last; an exact host adds no such row;
   `-x` shows "a host cannot start with '-'".
6. ↑/↓ wrap at both ends and scroll past 8 rows; Tab or a click toggles "Keep running on the host (tmux)"; a
   "recent · tmux" row shows it ticked.
7. Enter opens the host as a new session under its host group; ⌥Enter opens with tmux; clicking a row opens it.
   Escape or a click outside closes the picker; ⇧⌘S while open only refocuses it.
8. Repeat 3–7 in a light and a dark theme: everything stays legible. Type with an IME and note what ↑/↓ do mid-composition.
9. Log in (password or 2FA prompts appear in the pane). ⌘T, ⌘D and the group's "+" each open another pane to the
   same host at once, with no second login.
10. A wrong host shows ssh's error and the reconnect line instead of vanishing. With the tmux option on a host
    without tmux you see the error plus `ssh ended (exit 127) — press Enter to reconnect`, and no auto-reconnect.
11. Environment: from Finder, a host needing a Homebrew ProxyCommand or an agent set in `.zshrc` connects as in a
    terminal; `echo $SSH_AUTH_SOCK` shows the forwarded agent; the first SSH open is at most about 3 s slower.
12. Turn Wi-Fi off: within about a minute the pane keeps its screen and shows `Disconnected from HOST — press Enter
    to reconnect`, the ring turns done, and one "Finished" notification arrives. Other keys do nothing. Wi-Fi on,
    Enter: reconnected in the same place, on a fresh screen.
13. Drop while `vim` or `htop` runs remotely: the line appears on the normal screen in normal colours.
14. With tmux: the line says "reconnecting in 3s, or press Enter" and fuxx reconnects by itself to the same tmux
    session. With the network still down the waits grow (6, 12 … 60 s) and only the first drop notifies; closing
    the pane stops the retries. A connection that was up over a minute notifies again and starts at 3 s.
15. Two tmux panes to one host are independent (`tmux ls` on the host shows two `fuxx-<host>-…` names), and each
    reconnects to its own.
16. Focus during an auto-reconnect: an open picker or rename box keeps its typing; focus stays in the pane you are
    typing in; a focused held pane gives focus to the new one; ⌘T, the picker, splits and reopen still focus the
    new pane.
17. `exit` in a session that ran over 10 s closes the pane (including in the first pane to a host, not held open by
    the background shared connection); an `exit 1` within 10 s holds it (⌘W closes).
18. Collapse the host header, quit and reopen: still collapsed. With `session-restore = layout` or `agents`,
    saved SSH sessions reconnect; with a hand-edited bad host (`-oProxyCommand=x`) that entry doesn't start and the
    sidebar says why.
19. Persistence on (M10a): quit and reopen; the SSH session is still connected under its host. With an old
    background service still running, opening an SSH session says to restart it (`fuxx daemon stop`).
20. `fuxx new --ssh HOST [--tmux] [--split right --of sN] [--background] [--name X]` prints an id and
    `fuxx ls` shows `[ssh:HOST]`; `fuxx send` to a held session answers "disconnected — reconnect first".
21. Optional: `kill -STOP` a ControlPersist master, then open another pane to that host: it opens on a fresh
    connection after about 3–5 s, and the old socket in `/tmp/fuxx-<uid>/` is gone.

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
