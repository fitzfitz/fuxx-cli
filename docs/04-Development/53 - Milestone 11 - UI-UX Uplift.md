---
tags: [development, milestone]
status: built
---

# 53 - Milestone 11 - UI/UX Uplift

**Status:: 🟨 Built and reviewed (2026-11-06); on-screen check pending.**  ·  **Phase 4: fuxx looks and feels like a finished tool.**
**Plan:** [[M11 Plan - UI-UX Uplift]] · **Evidence:** [[M11 Input - UI-UX Review]]

## Goal

Make fuxx **fun to live in, without stopping being a professional tool**. Today it works but looks like a prototype:
a grey system titlebar, a terminal touching the window edges, same-looking sidebar rows, a 10 px dot as the only
"an agent needs you" signal, and light themes that fail contrast. After M11 you can tell at a glance which agent
needs you and what it's asking, the window feels designed, and small moments of motion make it feel alive.

## What shipped

Built on branch `m11-ui-ux` (plan: [[M11 Plan - UI-UX Uplift]]), every task reviewed, plus a whole-branch review and a
polish pass. 546 tests. Not yet run on screen: you will check M10b, M10c and M11 together (see below). In plain English:

- **The window:** no grey titlebar (the sidebar runs up under the traffic lights), padding around the terminal, a
  1280 × 800 default, and full View / Window menus (⌘\, ⌘= ⌘− ⌘0, ⌘M, ⌘H).
- **The sidebar:** clear rows with number and time in state, an accent bar on the active one, a tinted waiting row
  that shows the question, proper group headers, scrolling, a resizable and hideable sidebar, a "+ New" footer and a
  first-run hint card.
- **Status marks with shapes:** hollow ring (idle), spinning arc (working), pulsing dot (waiting), dot with a check
  that pops once (done). Animations Off or macOS Reduce motion stops all motion.
- **Splits:** a 22 px header per pane (mark, number, name, time), an accent underline on the focused one, quiet
  dividers that light up on hover and drag, a darker veil on unfocused panes.
- **Getting around:** ⇧⌘U jumps to what needs you; ⌘P is a session switcher sorted by urgency.
- **Smaller fixes:** rename selects the old name; notifications say `#3 api — …` with the question or "Worked for
  3m" as their body; the window title is `fuxx — #3 name`.
- **Preferences:** a two-column form with theme swatches, fonts in their own face, a live terminal preview and the
  Animations switch.
- **Light themes:** every UI colour is derived from the theme with a contrast floor, checked by a test over every
  built-in theme.
- **Screenshots:** `cargo run --features ui-shots -- __ui-shots` renders every scene off-screen to
  `target/ui-shots/` in a dark and a light theme.
- No new crates. The terminals, sessions, scripting, SSH and browsers behave as before.

## Your direction

- 2026-11-05: "I want to make the UI and UX fun and not boring but keep it professional."
- Earlier (M11 input): animations must be **switchable off**.
- 2026-11-05: "do it autonomously until M11 done". **Every choice below is the AI's call**, based on the review in
  [[M11 Input - UI-UX Review]] (its open questions are answered here, marked as the AI's call). Say if any should
  change.

## What "fun but professional" means here (AI's call)

- **Fun:** motion that means something (a spinning arc while an agent works, a soft pulse while it waits for you, a
  small "pop" when it finishes), one confident accent colour, friendly words in empty states, the 🖕 default name
  kept as fuxx's personality.
- **Professional:** quiet chrome, clear hierarchy, every theme readable (light ones too), motion only where it
  carries information, no sounds, no confetti, nothing that drains the battery, and one switch to turn all motion
  off.

## How it behaves

### The window

- **No grey titlebar.** The window's top is the app: the sidebar runs up under the traffic lights, the terminal goes
  edge to edge with padding, and the whole window takes the theme's colours. Drag the top band to move the window;
  double-click it to zoom, as macOS does.
- **Padding** around the terminal (10 px sides, 8 px top/bottom; less in splits), with leftover space shared evenly.
- **Full macOS menus:** Edit (from M10c), View (Toggle Sidebar ⌘\, Bigger/Smaller/Actual Size ⌘+ ⌘− ⌘0), Window
  (Minimize ⌘M, Zoom), and ⌘H. Default new-window size 1280 × 800.

### The sidebar

- **Rows that read at a glance:** 28 px rows (42 px for any row with a second line: a note, a progress bar, "not live" or a question); the name on one line (long names end with "…"); the session number in a
  small fixed-width column; the time in its current state on the right ("4m", "32s").
- **Which one is active:** the active session has a filled row and a 2 px accent bar; sessions visible in splits get
  a small marker instead of a fill.
- **Waiting stands out:** a waiting row gets a soft tint of the waiting colour, and the status mark is bigger.
- **"What is it asking?":** while a session waits, the question it's showing (for example "Do you want to make this
  edit to auth.rs?") appears under its name in small text. The row is 42 px tall while it has that second line (AI's call: yes, this is
  on-mission; any row with a sub-line is 42 px, not only waiting ones).
- **Group headers** look like headers: small caps (OpenType small caps; GPUI has no letter spacing at the pinned rev), a count, the "+" only on hover, and the group's ring
  only when collapsed.
- **Scrolls** when there are more sessions than fit, and keeps the active one in view.
- **Resizable** (180–400 px, remembered) and **can be hidden** (⌘\).
- **Footer:** "+ New" (with the presets) and a gear for Preferences. A first-run hint card explains the three things
  to know (⌘T, ⇧⌘S/⇧⌘B, the rings), dismissible for good.

### Status marks (the rings)

- Shapes, not just colours, so colour-blind users can tell them apart:
  - **Idle:** a small hollow ring.
  - **Working:** an arc that spins (gently, 10 frames a second, so it doesn't drain the battery).
  - **Waiting for you:** a solid dot with a soft pulsing halo.
  - **Done:** a solid dot with a check; it "pops" once when the agent finishes.
- Colours come from the theme with a guaranteed contrast floor.

### Splits

- **Pane headers** (22 px) when a session is split: status mark, number, name, time in state; the focused pane's
  header has the accent; a waiting pane's header has the waiting tint. (AI's call: yes, worth 22 px.)
- **Dividers** are 1 px and quiet (held to 1.4:1 against the background, on purpose), turning into a 2 px accent line on hover and while dragging.
- Unfocused panes dim a little more than today (0.3 → 0.4 veil).

### Getting around

- **⇧⌘U: "take me to what needs me"** — jumps to the session that has waited longest (then the next one each press).
- **⌘P: session switcher** — a small list with a filter box, sorted by urgency (waiting first, then finished, then
  working, then idle), showing each one's status and question.

### Smaller fixes

- Renaming selects the old name, so typing replaces it.
- Notifications and the window title include the session's number and group ("#3 api — Needs your input").
- Dragging a row shows a lifted preview, dims the original, and draws an insertion line.

### Preferences

- Redesigned as a two-column form, with theme swatches (each theme's colours as a little strip), fonts shown in their
  own face, and a live preview of a terminal in the chosen theme and font.
- **Appearance → Animations: On / Off** (and the macOS "Reduce motion" setting is respected either way). Off means
  no spinning, pulsing, popping or animated transitions anywhere.

### Light themes

- Every UI colour is derived from the theme with a contrast floor (text at least 4.5:1; status marks and accent/focus
  lines at least 3:1; plain dividers and panel borders are deliberately quiet at 1.4:1), checked by a test over every built-in theme, so light themes (like Catppuccin Latte) read properly.

## Answers to the review's open questions (AI's calls)

1. Unnamed sessions keep 🖕 as their name; notifications and titles gain the number and group. (No process lookup.)
2. The titlebar is hidden fully (integrated), as in Warp, Zed and cmux.
3. Pane headers: yes.
4. "Done" stays as today (no "unseen until viewed" change to the detector) — the pop and the time-in-state carry it.
5. The waiting question subtitle: yes. Hover peek: not in M11.
6. Motion budget: Working spins at 10 fps; Waiting pulses; everything stops with Animations off.
7. Light themes are a real target: contrast work comes first.

## Not in M11

- Hover peek, the attention-queue section, Dock badge, overview grid (signature ideas #2, #3, #5 in the review).
- A command palette with every action; a theme marketplace; icons on rows; sounds; vibrancy/blur.
- Changing what the status detector decides.

## Decisions made along the way

Rulings taken during the build and review (AI's calls, in keeping with the plan). The ones you would notice:

- **Two contrast levels.** Plain 1 px dividers and panel borders are held to a quiet 1.4:1; accent and focus lines and
  the status marks stay at 3:1 or more; text is 4.5:1 or more everywhere, hover backgrounds included.
- **One switch for all motion.** The Animations setting and macOS Reduce motion both turn off every animation
  (spinning, pulsing, the Done pop, the chevron turn). Marks are then drawn still.
- **Marks stop when the window can't be seen.** Minimised, hidden or fully covered: no redraws, so about 0 % CPU.
- **Terminals are cached.** A mark tick repaints the sidebar and headers, not the terminal grids.
- **Age labels ("12s", "4m") refresh coarsely**: about once a second for a Done row's first minute, then per minute,
  then per hour, never while the window is hidden. That is a wake, not an animation.
- **Group headers use small caps**, because GPUI has no letter spacing at the pinned rev.
- **Any row with a second line is 42 px** (note, progress, "not live", question), not only waiting rows.
- **One overlay at a time.** Opening ⌘P, ⇧⌘S, ⇧⌘B or a rename closes whatever box was open and keeps the keyboard.
- **The waiting question is cleaned:** box-drawing characters and surrounding spaces are trimmed, and a lone
  `(y/n)` borrows the line before it. A prompt that looks like a secret (password, passphrase, token) shows nothing.
- **Notification body = the question** (or "Needs your input"); for Done it is "Worked for <time>" (or "Finished").
  The summary stays `#<n> <group or name> — <message>`. The `fuxx events` notification carries the same text.
- **Preferences is 760 × 646 and not resizable**, with "Keep running after quit" under a new Sessions section.
- **Browser windows ignore ⌘= ⌘− ⌘0**, so they never resize the terminals behind them.
- **The rename selection is a clear accent tint** (not the old faint grey), readable in Latte too.
- **Dragging a window** by the top band; double-click follows the macOS title-bar setting (zoom or minimise).
- **Screenshots** come from a hidden entry point because AppKit needs the main thread: `cargo run --features
  ui-shots -- __ui-shots [filter]` (PNGs land in `<repo>/target/ui-shots`).
- **Sidebar state is saved** in `sessions.json` (width, hidden, hint dismissed), not in the config file.

## Known limitations

- Screenshots come from GPUI's off-screen renderer; the real window can differ slightly. The on-screen check is the
  authority.
- Agent-browser windows (raw NSWindow) don't respond to Window › Minimize / Zoom or ⌘M; their own buttons work.
- `done_tint` is defined but never drawn; a hovered Done row's check doesn't match the hover fill.
- Sidebar numbers read "3" while the switcher and pane headers read "#3".
- The Preferences window uses the system titlebar and a different selection fill from the main window.
- A session adopted while already Waiting shows "Needs your input" until its next change, not the question.
- The address-box error line (on a dark theme's panel) is only guaranteed 3:1, not 4.5:1.
- Text that sits at exactly 4.5:1 has no margin; some dim and faint greys look alike on Dracula, Nord, Gruvbox and
  Latte.
- At very large font sizes the Preferences preview crops ligature samples and the `ls` line.
- ⇧⌘− and numpad zoom keys aren't swallowed in browser windows (they are unbound, so harmless).
- Nothing marks the drag band when the sidebar is hidden (same as Ghostty and Zed).
- Hover peek, the attention queue, Dock badge and overview grid are later ideas.
- Smaller items are in the plan's Deferred minors.

## On-screen checklist (pending)

Run these together with the M10b and M10c checks.

1. Run fuxx from the .app opened in Finder (not from a terminal) for all checks below.
2. No grey titlebar; the traffic lights sit at the sidebar's top-left. Drag the top band: the window moves.
   Double-click it: it zooms or minimises per your macOS setting. Full screen leaves no empty band.
3. ⌘\ hides and shows the sidebar (and ends a rename); with it hidden a 38 px band above the terminal still drags
   the window. The size and hidden state come back after a restart.
4. Terminal text has a margin (a little less in splits). Click, double-click, ⌘-click a link and `vim` with mouse
   reporting hit the cell under the pointer, including in the top rows under the transparent titlebar.
5. ⌘= ⌘− ⌘0 change the font size in every session and in Preferences; ⌘M minimises, ⌘H hides.
6. Drag the sidebar's right edge (180–400 px): the accent line shows and the width is remembered. The first-run hint
   card shows and its × dismisses it for good.
7. Rows: active row has a fill and accent bar; a waiting row is tinted and shows the question; ages tick; a group
   header collapses with a turning chevron; "+" shows on hover; a long list scrolls and keeps the active row in view;
   drag-reordering shows a lifted row and an insertion line.
8. A Working arc spins, a Waiting dot pulses, a Done dot pops once (collapse and expand must not replay it).
9. Preferences → Animations Off, then (separately) System Settings → Reduce motion: all motion stops; On or off
   again and it moves again.
10. Minimise, ⌘H or fully cover the window while an agent works: Activity Monitor shows fuxx at about 0 % CPU; restore
    it and the arcs spin at once. With another app in front but fuxx still visible, they keep spinning.
11. While marks spin: typing, output, a selection drag, scrolling and ⌘-hover on a link repaint instantly; split and
    unsplit re-pad correctly; a theme or font change repaints the terminals.
12. Splits: headers appear on a split and vanish at one pane; clicking a header focuses that pane (also from the
    active one, taking the keyboard back from a rename box); the focused header has the accent underline; unfocused
    panes are dimmer.
13. Dividers are quiet; hover gives a 2 px accent line that stays lit during a drag and clears after a layout change.
14. ⇧⌘U with several Waiting and Done sessions: goes to the longest-waiting, then the next each press, then wraps;
    with none, nothing happens.
15. ⌘P: most urgent row selected; typing filters (name, number, group, question); ↑/↓ and Enter work; Esc, a click
    outside, or clicking the terminal closes it; typing then reaches the terminal.
16. Rename (⌘R or double-click a row): the old name is selected, clearly visible in a light theme, typing replaces it.
17. One overlay at a time: rename open then ⌘P; ⌘P open then rename; ⌘P then ⇧⌘B; ⇧⌘S over ⌘P. The new box keeps the
    keyboard. The picker and address box look like ⌘P.
18. Notifications: a background session turning Waiting reads "#3 api — Needs your input" over the question; Done
    reads "… — Finished" over "Worked for 3m". The window title is `fuxx — #4 shell`.
19. Preferences (⌘,) is about 760 × 646 with nothing clipped: theme swatches match their themes, fonts are drawn in
    their own face, the live preview follows theme, font, ligatures and size, including 16–20 pt.
20. In an agent-browser window press ⌘= ⌘− ⌘0: the terminals' font size does not change (it still does in the main
    window).
21. Latte everywhere: sidebar, headers, hover rows, switcher, panels, Preferences and the rename box stay readable
    (Nord too for hover text).

## Success check

1. A dark and a light theme both look designed and readable: titlebar gone, padding, clear active row.
2. With three agents running, one waiting: you can see which one waits, and what it's asking, without clicking.
3. ⇧⌘U takes you to it; ⌘P lists sessions most-urgent first.
4. Working spins, Waiting pulses, Done pops; Animations → Off stops all of it.
5. A split shows pane headers with the focused one marked.
6. Preferences shows theme swatches and a live preview.


## Amendment (2026-11-06, your request): Preferences as vertical tabs

Plan: [[M10c-M11 Follow-up Plan - Browser Bar and Tabbed Preferences]]. Replaces the one-page Preferences above.

- A list of sections down the left, like VS Code's settings; only the selected section's settings show on the right.
- Sections (your choice): **General**, **Appearance**, **Sessions**, **Browser**.
  - General: Animations, and "Show the tips card again" (AI's call: the only app-wide settings that exist today).
  - Appearance: Theme (with swatches), Font, Size, Ligatures, and the live preview.
  - Sessions: Restore sessions, Keep running after quit.
  - Browser: Show address bar, Keep history and data.
- The window keeps one size for all sections; it opens on the section you used last in this run (General first).

## Related

- [[40 - Roadmap]] · [[M11 Input - UI-UX Review]] · [[46b - Milestone 6b - Settings Window]] ·
  [[47 - Milestone 7 - Session Ergonomics]] · [[48 - Milestone 8 - Split Panes]]

## Amendment (2026-11-06, your requests): sidebar button and tab-like rows

Plan: [[M10c-M11 Follow-up Plan - Browser Bar and Tabbed Preferences]] (Tasks 3b, 3c).

- A sidebar button beside the window buttons hides/shows the sidebar (same as ⌘\); the sidebar slides with motion on.
- Rows behave like tabs: × on hover, middle-click closes, right-click menu (Close Tab, Close Other Tabs, Close Tabs to
  the Right, Show All Tabs, Rename Tab…, Tab Color), bulk closes always ask first. The number column goes; the right
  side shows a colour dot and "⌘N" (your sample). "Move Tab to New Window" waits for multi-window support.

