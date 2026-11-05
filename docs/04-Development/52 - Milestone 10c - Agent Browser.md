---
tags: [development, milestone]
status: planned
---

# 52 - Milestone 10c - Agent Browser

**Status:: ⬜ Planned (spec written 2026-11-06).**  ·  **Phase 3: a browser your agents can drive, beside the terminal.**
**Plan:** [[M10c Plan - Agent Browser]] · **Evidence:** [[Input - M10 Feasibility Study]] §4

## Goal

Each session can have its own browser window. The agent in that session can open pages, read them, click, type,
run JavaScript and take screenshots through `fuxx` commands and `fuxx mcp`, while you watch the same window. Typical
use: an agent starts a dev server, opens `localhost:3000` in its browser, checks the page, fixes the bug, and checks
again.

## Your choices

- 2026-10-27: the browser is **for agents to automate** (like cmux), not only a preview.
- 2026-11-05: "do it autonomously until M11 done". **Every choice below is the AI's call**, based on the feasibility
  study. Say if any should change.

## How it behaves

### The window

- A session's browser is a **separate window** beside fuxx, not a pane inside it (AI's call, study §4.4). A web page
  drawn inside fuxx's window would sit on top of everything fuxx draws there (menus, the rename box, the host
  picker) and fight it for the keyboard; a window of its own avoids all of that.
- Title: `<session name> — <page title>`. A plain web view: no tabs, no address bar. Normal macOS editing keys work
  in it (⌘C, ⌘V, ⌘A, ⌘Z); ⌘W closes it.
- One browser per session. Closing the session closes its browser. Closing the browser window leaves the session.
- Logins and cookies are shared by all fuxx browsers and kept between runs.
- Safari's Web Inspector can attach to it (Develop menu), for when you want dev tools.

### Opening it

- **File → Open Browser… (⇧⌘B)** asks for an address (a small box like the host picker) and opens the active
  session's browser there. If it is already open, it goes to that address and comes to the front.
- From scripts and agents: `fuxx browser open URL` (the session in `$FUXX_SESSION`, or `--session sN`).
- Addresses: `http://` and `https://` only. A bare `localhost:3000` or `example.com` gets a scheme added (`http://`
  for localhost and IP addresses, `https://` otherwise). Anything else (`file:`, `javascript:`, `data:`) is refused.

### What agents can do

All through `fuxx browser <command>` and the same tools in `fuxx mcp`, for the session's own browser:

| Command | Does |
|---|---|
| `open URL` | Open the browser (if needed) and go to URL; waits until the page has loaded (30 s limit). |
| `snapshot` | A short outline of the page: headings, links, buttons, fields and text, each control with a `ref` like `e12`. |
| `click REF` | Click that element. |
| `fill REF TEXT` | Put text into that field (and tell the page it changed). |
| `press KEY` | Press a key in the focused element (Enter, Tab, Escape, arrows). |
| `read` | The page's visible text. |
| `eval JS` | Run JavaScript in the page and return the result as JSON. |
| `wait --text T` / `wait --selector S` | Wait until it appears (30 s default limit). |
| `screenshot [PATH]` | Save a PNG of the page (default: a temp file; prints the path). |
| `url` | The current address and title. |
| `back` / `reload` / `close` | As named. |

- Refs come from the latest `snapshot` and are good until the page changes.
- Every command fails clearly when there's no browser, the ref is unknown, or the page didn't answer in time.

### With other milestones

- **M9 scripting:** the same private socket and the same rules: only your macOS user can reach it.
- **M10a background service:** browsers belong to the window, not the service. With fuxx closed, browser commands
  answer that the fuxx window isn't open (like other window-only commands), and browsers don't come back when fuxx
  reopens.
- **M10b SSH:** a browser opens addresses from your Mac. A dev server on a remote host is reached through your own
  port forwarding (fuxx doesn't forward ports in M10c).

### Limits to know

- **Powerful by design.** An agent can run any JavaScript in pages it opens and use your saved logins in fuxx's
  browser, the same as with any agent browser tool. Only programs running as your macOS user can ask.
- No tabs, downloads, file uploads or pop-up windows (a page's `window.open` loads in the same window).
- Browsers aren't restored after quitting fuxx.
- macOS only (WebKit).

## Rough scope / checklist

- [ ] Task 0 check: a web view can be made, loaded, scripted and captured from fuxx's process (a hidden probe).
- [ ] A browser window per session, opened from the menu (⇧⌘B) with an address box.
- [ ] Commands: open, snapshot, click, fill, press, read, eval, wait, screenshot, url, back, reload, close.
- [ ] The same in `fuxx mcp`.
- [ ] Closes with its session; shared, kept logins; Web Inspector allowed.

## Not in M10c

- A browser inside a pane, tabs, an address bar, downloads/uploads.
- Port forwarding for SSH sessions.
- Restoring browsers after quitting.
- Browser commands while the window is closed.

## Success check

1. In a session, `fuxx browser open localhost:3000` (a running dev server): the browser window appears with the page.
2. `fuxx browser snapshot` lists the page's buttons and fields with refs; `fuxx browser click e3` clicks one and the
   page reacts on screen.
3. `fuxx browser fill e5 "hello"` then `fuxx browser press Enter` submits a form.
4. `fuxx browser screenshot` saves a PNG that matches the window.
5. From `claude` with `fuxx mcp` connected, ask it to open the page and check a heading: it does, through the tools.
6. ⌘W on the session closes its browser too.

## Related

- [[40 - Roadmap]] · [[Input - M10 Feasibility Study]] · [[49 - Milestone 9 - Scripting API]] ·
  [[50 - Milestone 10a - Persistent Sessions]] · [[51 - Milestone 10b - SSH Workspaces]]
