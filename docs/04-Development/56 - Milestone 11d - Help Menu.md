---
tags: [development, milestone]
status: built
---

# 56 - Milestone 11d - Help Menu

**Status:: 🟨 Built and reviewed 2026-11-08 (786 tests); on-screen check pending.**  ·  **Phase 4: an M11 extension.**  ·  **Plan:** [[M11d Plan - Help Menu]]
**Input:** [[M11d Input - Help Menu]] (your request and choices, 2026-11-06).

## Goal

A **Help** menu after Window, like Ghostty's. It holds a searchable list of every fuxx shortcut, a searchable
reference for the `fuxx` commands, the notification status, a way to bring back the welcome tips, and Report an
Issue / Show Logs.

## Why now (your choices)

- 2026-11-06: "add a help menu" with a searchable **Shortcuts** list. Asked what else belongs there, you chose all four
  suggestions: the command reference, the notifications status, showing the welcome tips again, and report an issue /
  logs.
- 2026-11-07: "plan it", then "go" (skip the questions; AI designs it from the input note). **So every design choice
  below is AI's call** unless marked yours. Correct any of them before the build.

## The menu

```
Help
  [Search]                    ← added by macOS to the Help menu (finds menu commands)
  Keyboard Shortcuts      ⌘/
  fuxx Command Reference
  ─────────────
  Notifications Status…
  Show Welcome Tips
  ─────────────
  Report an Issue…
  Show Logs
```

- It comes after Window. fuxx marks it as the app's Help menu (`NSApplication.helpMenu`), so macOS adds its search
  box. GPUI marks only the Window menu, so fuxx does this itself every time the menu bar is rebuilt.
- **⌘/** opens Keyboard Shortcuts (AI's call, as in many Mac apps). macOS already uses ⇧⌘/ to open Help's search.

## Keyboard Shortcuts (⌘/)

- A **panel over the window**, like ⌘P: same surface, filter box at the top, list under it, a hint line at the
  bottom. It is wider than ⌘P (560 px) because each row has a name on the left and keys on the right.
- **Built from the real key-binding table** (`key_bindings()` in `main.rs`, browser page keys included), so the list
  can't drift. A test fails when someone adds a binding without a name for this list or an explicit "not listed" mark.
- **Grouped:** Workspaces · Tabs · Panes · Terminal · Browser pages · Window & app. Within a group, rows keep the
  order of the binding table.
- **Keys shown the Mac way:** `⇧⌘D`, `⌃⌘=`, `⌘←`, `⇞`/`⇟` (Page Up/Down), `⌃⇥`. One action with several keys shows
  them as `⌘= / ⌘+`. Numbered runs collapse: Go to workspace 1–9 `⌘1 – ⌘9`; Start agent preset 1–9 `⌃⌘1 – ⌃⌘9`.
> [!note] M11l (2026-11-09) added three rows to the **Terminal** group: **Previous command** (⇧⌘↑), **Next command** (⇧⌘↓) and **Copy command output** (⇧⌘C). See [[63 - Milestone 11l - Command Blocks]].
- **Search** matches a row's name, its group or its keys, ignoring case. "split" finds Split Right and Split Down;
  "⌘d" and "cmd d" both find Split Right. With nothing found it says "No shortcut matches".
- **Read only.** Enter does nothing and clicking a row does nothing. ↑/↓ scroll the list. **Esc**, a click outside,
  or **⌘/ again** closes it, and the keyboard goes back to the focused tab.
- Not listed: keys that only work inside a box (Enter/Esc/⌫ in rename, ↑/↓ in the pickers), the bindings that exist
  only to label the Edit menu, and ⌃⌘Space (Emoji & Symbols).

## fuxx Command Reference

- **The same panel**, on its other page: placeholder "Search commands", one row per command. Each row shows the usage
  in the terminal font (one or more lines, as `fuxx help` prints it) with the description under it, dimmer.
- **Built from `fuxx help`'s own text** (`cli::USAGE`), split into entries, so the two can't disagree. A test checks
  that every command `fuxx` knows has an entry.
- **Grouped:** Sessions (ls, new, close, focus, rename) · Driving a session (send, key, paste, read, status, question,
  answer, wait, events) · Sidebar and banners (note, progress, notify) · Layout (equalize, presets) · Browser
  (browser …) · fuxx itself (ping, version, mcp, daemon …, help).
- **An "MCP setup" group at the end:** `claude mcp add fuxx -- <path>` with the path of the fuxx that is running
  (e.g. `/Applications/fuxx.app/Contents/MacOS/fuxx`; single-quoted if the path has spaces), so it can be typed or
  pasted as it stands. The panel's text can't be selected, so there is no copying from it (see Not in M11d).
- The usage text's closing notes (`--json`, what TARGET is, the exit codes) are the page's footer.
- Search matches the usage, the description or the group. Esc / click outside closes it. Unlike Keyboard Shortcuts,
  it has no key of its own.

## Notifications Status…

A macOS sheet on the window (GPUI's `prompt`, as the close-tab question uses). It reads macOS's current answer for
fuxx when you choose the item. The read happens off the UI thread and waits at most 2 s (`native_notify::permission`).

| Situation | Title | Text | Buttons |
|---|---|---|---|
| Allowed | fuxx sends its own notifications | Banners come from fuxx, and clicking one shows the terminal that raised it. | OK · Open System Settings |
| Denied or turned off | fuxx's own notifications are off | macOS doesn't allow fuxx's banners, so fuxx sends Script Editor banners instead, without click-through. Turn fuxx on in System Settings → Notifications. | Open System Settings · OK |
| Not answered | Notifications not answered yet | Until you allow fuxx, it sends Script Editor banners instead. | Open System Settings · OK |
| Not the app (`cargo run`) | Not running as fuxx.app | This fuxx was started outside its app, so macOS can't give it notifications of its own. It sends Script Editor banners instead. | OK |
| No answer in 2 s | Couldn't read the notification setting | macOS didn't answer in time. Try again, or look in System Settings → Notifications → fuxx. | Open System Settings · OK |

"Open System Settings" opens `x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=io.github.fitzfitz.fuxx`
(fuxx's own page). Whether macOS opens fuxx's page or only the Notifications list is checked on screen; either is
acceptable.

## Show Welcome Tips

> [!note] Renamed **Show Tips Card** in M11k (2026-11-09), when Help gained **Welcome to fuxx** (note 62). The action is unchanged.

Brings back the sidebar's first-run tips card and remembers that it is shown. This is the same action as Preferences'
"Show the tips card again". If the sidebar is hidden, it is shown too, because the card lives in it.

## Report an Issue…

Opens your browser on a new GitHub issue for fuxx (`https://github.com/fitzfitz/fuxx-cli/issues/new`, the public
repo). The issue text is filled in in advance:

```
**What happened**


**What you expected**


---
fuxx 0.x.y · macOS 26.x
```

The version is fuxx's own. The macOS version comes from `sw_vers -productVersion`, read off the UI thread; if that
fails it says "macOS unknown". Nothing else is sent: no logs and no settings.

## Show Logs

- Opens `~/Library/Logs/fuxx/` in Finder, creating it if it isn't there. The background service already writes
  `daemon.log` there.
- **New: the window writes `fuxx.log` there too**, but only when it runs from `fuxx.app` and its error output isn't
  a terminal. Opened from Finder or the Dock, its messages went nowhere until now. A `cargo run` in a terminal keeps
  printing to the terminal. At launch, a `fuxx.log` over **1 MiB** is started fresh; otherwise new lines are appended.
  The first line of each run is `fuxx <version> started (pid N)`.

## As built (2026-11-08)

Where the build settled something the plan left open, or changed it:

- **⌘/ from another window** (Preferences, a popped-out page) brings the main window forward with Keyboard Shortcuts
  open; it closes the panel only when the main window was already in front. The Command Reference item follows the
  same rule. (AI's call while building: pressing ⌘/ elsewhere means "show me".) So `open_help` also takes whether the
  main window was key.
- **⌥⌘Q** ("Quit fuxx and End All Sessions", bound only with persistent sessions on) is listed whenever it is bound.
  Its binding moved into `persistent_bindings()`, which both the app and the list read. The list also reads ⌘N/⌘T
  (`new_bindings()`), and the "every shortcut is listed" test covers all three tables.
- Every Help item is an app-level action. The ones that need a window go to the main fuxx window (`help_target`), so
  they work while Preferences or a popped-out page is in front. Report an Issue and Show Logs need no window.
- An open Help panel hides the in-pane browser pages, as the other panels do (otherwise the web page would cover it).
- `show_welcome_tips` takes the window, because it reuses Toggle Sidebar. The menu bar is built by `menu_bar()`, so
  its order is tested on the real bar.
- `fuxx.log` is written only for the window (`fuxx` with no command), never for `fuxx <command>`, `fuxx mcp` or the
  background service.

## Not in M11d

- Running a shortcut or copying a command from the panels. They are reference only.
- Help pages and docs inside the app, a website, or release notes.
- Sending logs automatically or attaching them to an issue.
- Changing shortcuts (no key remapping).
- Rotating `daemon.log` (it stays as it is).
- Greying out "Show Welcome Tips" while the card shows (GPUI's menu has no live enabled state).

## On-screen checklist (pending)

Run `target/dist/fuxx.app` (the bundle, so notifications and the log behave as shipped).

1. The menu bar reads fuxx · File · Edit · Pane · View · Window · **Help**.
2. Help has a search box at the top, and typing "split" in it highlights Pane → Split Right.
3. Help lists, in order: Keyboard Shortcuts ⌘/, fuxx Command Reference, —, Notifications Status…, Show Welcome
   Tips, —, Report an Issue…, Show Logs.
4. ⌘/ in a terminal opens Keyboard Shortcuts over the window, with the caret in its box. Nothing is typed into the
   terminal.
5. The groups read Workspaces, Tabs, Panes, Terminal, Browser pages, Window & app. Keys look like ⇧⌘D, ⌘1 – ⌘9 and
   ⌘= / ⌘+.
6. Typing "split" leaves Split Right and Split Down. "zzz" shows "No shortcut matches".
7. Esc closes it and you can type in the terminal again. Doing the same with ⌘/ and with a click outside works too.
8. Help → fuxx Command Reference opens the same panel showing commands. Typing "browser" leaves the browser commands.
   `new`'s usage shows on its two lines, with its description under them.
9. The MCP setup row shows `claude mcp add fuxx -- /…/fuxx.app/Contents/MacOS/fuxx`.
10. Notifications Status…, with fuxx allowed, shows "fuxx sends its own notifications". Open System Settings opens
    Notifications (fuxx's page or the list).
11. Turn fuxx's notifications off in System Settings. Notifications Status… now says "…are off". Turn them back on.
12. Close the tips card (×), hide the sidebar (⌘\), then Help → Show Welcome Tips: the sidebar comes back with the
    card.
13. Report an Issue… opens a new GitHub issue on fitzfitz/fuxx-cli, with the text filled in and the right fuxx and
    macOS versions.
14. Show Logs opens `~/Library/Logs/fuxx/` in Finder. `fuxx.log` is there, and its last run starts with
    `fuxx <version> started`.
15. Light and dark themes: both panels are readable (the ui-shots cover this too).
16. With Preferences in front, ⌘/ brings the main window forward with Keyboard Shortcuts open; ⌘/ again (now in
    front) closes it.
17. With "keep sessions running" on, Keyboard Shortcuts lists ⌥⌘Q "Quit fuxx and End All Sessions".
