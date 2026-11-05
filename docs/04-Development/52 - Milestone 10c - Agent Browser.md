---
tags: [development, milestone]
status: built
---

# 52 - Milestone 10c - Agent Browser

**Status:: 🟨 Built and reviewed (2026-11-06); on-screen check pending.**  ·  **Phase 3: a browser your agents can drive, beside the terminal.**
**Plan:** [[M10c Plan - Agent Browser]] · **Evidence:** [[Input - M10 Feasibility Study]] §4

## Goal

Each session can have its own browser window. The agent in that session can open pages, read them, click, type,
run JavaScript and take screenshots through `fuxx` commands and `fuxx mcp`, while you watch the same window. Typical
use: an agent starts a dev server, opens `localhost:3000` in its browser, checks the page, fixes the bug, and checks
again.

## What shipped

Built on branch `m10c-agent-browser` (plan: [[M10c Plan - Agent Browser]]), every task reviewed, plus a whole-branch
review and a fix pass. 501 tests; the hidden `fuxx __browser-probe` (a real off-screen web view driven end to end)
passes. Not yet run on screen: you will check M10b, M10c and M11 together (see below).

- A WebKit browser window per session (`browser.rs`), scripted through channels, with load errors reported by
  WebKit's own reason.
- Addresses: http(s) only, a scheme added to bare hosts (`browser_url.rs`).
- Page-side scripts for snapshot (with `eN` refs), click, fill, press, read and wait (`browser_js.rs`).
- A browser per session in the workspace, File → Open Browser… (⇧⌘B) with an address box
  (`workspace_browser.rs`); it follows renames and SSH reconnects and closes with its session.
- `fuxx browser open|snapshot|click|fill|press|read|eval|wait|screenshot|url|back|reload|close` and 13 matching
  `fuxx mcp` tools (the tool count went from 14 to 27).
- A new Edit menu, and ⌘W / ⌘Z in the browser window.
- The probe stays in the code as a self-test for future GPUI or WebKit updates: `fuxx __browser-probe` (hidden from
  `fuxx help`) opens no visible window and exits by itself (0 = all checks passed, 2 = timed out).

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

## Rough scope / checklist (all built; on-screen check pending)

- [x] Task 0 check: a web view can be made, loaded, scripted and captured from fuxx's process (a hidden probe).
- [x] A browser window per session, opened from the menu (⇧⌘B) with an address box.
- [x] Commands: open, snapshot, click, fill, press, read, eval, wait, screenshot, url, back, reload, close.
- [x] The same in `fuxx mcp`.
- [x] Closes with its session; shared, kept logins; Web Inspector allowed.

## Not in M10c

- A browser inside a pane, tabs, an address bar, downloads/uploads.
- Port forwarding for SSH sessions.
- Restoring browsers after quitting.
- Browser commands while the window is closed.

## Decisions made along the way

Rulings taken during the build and review (AI's calls, in keeping with the plan). The ones you would notice:

- **A separate window per session**, not a pane (as the spec says).
- **An agent's `browser open` brings the window forward without taking the keyboard**, so it can't swallow what you
  are typing. Only ⇧⌘B (your own action) makes the browser the key window. If fuxx is behind other apps, an
  agent-opened browser may stay behind them too.
- **A new Edit menu** (Cut, Copy, Paste, Select All) so ⌘X/⌘C/⌘V/⌘A reach the page; in the terminal they behave as
  before (Copy and Paste work, ⌘A and ⌘X do nothing). **No Undo item in the menu**: GPUI routes those to itself, so
  the browser window handles ⌘Z and ⇧⌘Z directly. ⌘W in the browser window closes only the browser; it matches the
  key's own letter on Latin layouts and the US key position on other scripts, as the menus do, so ⌘W can never end
  a session from the browser on a non-Latin layout.
- **`target=_blank` links and `window.open` load in the same window.**
- **Load errors say why**: `the page failed to load: <WebKit's reason>` (for example "Could not connect to the
  server."). A page that arrives later clears the old error.
- **Pages with a strict content security policy work**: `eval` runs your code as an expression first and only
  falls back to statements when it does not parse, so it never runs twice.
- **Page text is printed without control characters** in the CLI's human-readable output (`read`, `url`, titles,
  `snapshot`, `eval`, error messages), so a page can't retitle your session or fire a notification. `--json` output
  stays exact.
- **The browser follows its session through an SSH reconnect**: it keeps its window, page and row.
- **`//host` and `/x` are refused** with "an address needs a host"; `javascript:`, `file:`, `data:`, `about:`
  (other than blank) and an empty port are refused with the http(s)-only message.
- **`wait` that times out says "timed out waiting"** and exits 124, as M9's `wait` does: the page answered, the
  condition just never held.
- **`snapshot` prints `title:` and `url:` lines** above the outline.

## Known limitations

- Powerful by design: an agent can run any JavaScript in pages it opens and use your saved logins in fuxx's browser.
- No tabs, downloads, file uploads or pop-up windows. No address bar or Back button in the window (use the
  commands).
- Browsers aren't restored after quitting fuxx; with the window closed, browser commands aren't available.
- With the browser window in front, fuxx's own menu shortcuts (⌘T, ⌘R, ⌘D, ⇧⌘B…) do nothing; ⌘Q and ⌘, still work.
  Click fuxx's window first.
- `eval` of a declaration (`var x = 1`) doesn't persist, and a Promise comes back as `{}` (JSON can't hold it).
- `open about:blank` reports a failed load.
- Commands in flight during an SSH reconnect answer "browser closed"; the browser itself carries on.
- `wait` can overrun its limit by up to 10 s on a hung page, and may give up early if a script errors mid-navigation.
- `back` / `reload` answer after about 200 ms, which assumes WebKit has started the navigation (unmeasured).
- Screenshots are PNG-encoded on the UI thread; the Retina scale is unchecked (the probe ran at 1.0).
- A page's refs can sit on recycled DOM nodes: take a new `snapshot` after the page changes.
- Smaller items are listed in the plan's Deferred minors.

## On-screen checklist (pending)

Run these together with the M10b and M11 checks. Needs a local dev server (for example `python3 -m http.server 3000`
in a folder with a page that has a form) and a second session.

1. Run fuxx from the .app opened in Finder (not from a terminal) for all checks below.
2. File menu has "Open Browser… ⇧⌘B"; the menu bar now has Edit (Cut ⌘X, Copy ⌘C, Paste ⌘V, Select All ⌘A) between
   File and Pane.
3. ⇧⌘B opens the address box (dim placeholder "Address — localhost:3000 or a URL"); Esc or a click outside closes
   it; typing goes into the box, not the terminal.
4. Enter `localhost:3000`: a 1100 × 800 window opens, cascaded from fuxx's, in front and key (typing goes to the
   page); within about 0.5 s the title is `<session name> — <page title>`.
5. Enter `javascript:alert(1)`: the box stays open with "only http and https addresses can be opened"; an empty
   Enter says "empty address"; `//x` says "an address needs a host".
6. ⇧⌘B again with the browser open: the box holds the current URL (selected); Enter reloads it and brings the
   window forward. ⌘V pastes into the box.
7. Rename the session (⌘R): the browser title follows within about 0.5 s.
8. In the browser: drag-select text, ⌘C, ⌘V into a field; ⌘A in a field and on the page; ⌘X; type then ⌘Z and
   ⇧⌘Z. (Edit items stay greyed until you click into the page once.)
9. In the terminal, ⌘C and ⌘V work as before and the Edit menu items do the same.
10. ⌘W with the browser in front closes only the browser (the session stays; ⇧⌘B then opens an empty box); ⌘W with
    fuxx in front closes the session and its browser; ⌘Q in the browser still quits fuxx. Closing it with the red
    button also leaves the session.
11. Try ⌘W and ⌘Z in the browser on a U.S., an AZERTY and a Russian layout: ⌘W must never end a session.
12. With the browser in front, ⌘T does nothing; click fuxx's window and it works again (known limitation).
13. `fuxx browser open localhost:3000` from the session: window appears and url and title print. On a dead port:
    "the page failed to load: Could not connect to the server." A link with `target=_blank` clicked by hand opens in
    the same window.
14. `snapshot` lists headings, links, buttons and fields with `eN` refs; `click eN` reacts on screen; `fill eN hello`
    then `press Enter` submits a form; `click e999` says "unknown ref e999 (take a new snapshot)".
15. `eval 'document.title'` prints the title; `read` prints the page text; `wait --text X --timeout 2` on missing
    text exits 124 after about 2 s ("timed out waiting"), on present text returns at once; closing the window during
    a `wait` answers "browser closed".
16. `screenshot` prints a `$TMPDIR/fuxx-browser-sN-….png` path that matches the window (check it looks sharp on
    Retina); `screenshot out.png` writes into the shell's folder. `back`, `reload` (watch that they answer after the
    page really moved), `url`, `close`.
17. Type in session 2's terminal while an agent in session 1 runs `fuxx browser open …` repeatedly: typing stays in
    the terminal. Minimise the browser and run `open` again: it comes back (unverified). ⇧⌘B still makes the browser
    key.
18. `fuxx browser read` on a page containing `\x1b]0;x\x07` text does not retitle the session.
19. A second session's browser is a separate window with its own page; closing a session closes only its browser.
20. SSH reconnect: with an SSH session's browser open, break the connection and reconnect with Enter. The browser
    keeps its window, page and name.
21. From `claude` with `fuxx mcp` connected, ask it to open the page and check a heading: it does, through the
    `browser_*` tools.
22. Safari → Develop → your Mac lists the fuxx page and Web Inspector attaches; a login on a site survives quitting
    and reopening fuxx.

## Success check

1. In a session, `fuxx browser open localhost:3000` (a running dev server): the browser window appears with the page.
2. `fuxx browser snapshot` lists the page's buttons and fields with refs; `fuxx browser click e3` clicks one and the
   page reacts on screen.
3. `fuxx browser fill e5 "hello"` then `fuxx browser press Enter` submits a form.
4. `fuxx browser screenshot` saves a PNG that matches the window.
5. From `claude` with `fuxx mcp` connected, ask it to open the page and check a heading: it does, through the tools.
6. ⌘W on the session closes its browser too.


## Amendment (2026-11-06, your request): a modern top bar, an address bar, and a keep-data setting

Plan: [[M10c-M11 Follow-up Plan - Browser Bar and Tabbed Preferences]]. This replaces "no address bar" and "logins
and cookies kept between runs" above.

- **Top bar:** macOS's own compact toolbar (as Safari): traffic lights inline, back / forward / reload buttons, a
  rounded address field that shows the current page and opens what you type (Enter; the same http/https check agents
  use; Escape puts the current address back; ⌘L focuses it), and a thin loading line. Light or dark follows the fuxx
  theme.
- **Preferences → Browser → Show address bar** (On by default). Off: the bar keeps back / forward / reload and the
  page title; only the address field goes. Applies to open browser windows at once.
- **Preferences → Browser → Keep history and data** (Off by default — your choice).
  - Off: each fuxx run starts from zero. Cookies, logins, site storage and cache live in memory only, shared by all
    browser windows in that run, and vanish when fuxx quits.
  - On: they're saved and come back next run.
  - Turning it from On to Off **deletes** what was saved (your choice). The change applies to browser windows opened
    after it; windows already open keep the store they started with until closed (AI's call).
  - "History" means each window's back/forward list and what sites store; fuxx keeps no separate list of visited pages.

## Related

- [[40 - Roadmap]] · [[Input - M10 Feasibility Study]] · [[49 - Milestone 9 - Scripting API]] ·
  [[50 - Milestone 10a - Persistent Sessions]] · [[51 - Milestone 10b - SSH Workspaces]]
