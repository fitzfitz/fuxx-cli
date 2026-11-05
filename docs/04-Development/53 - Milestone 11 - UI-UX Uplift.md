---
tags: [development, milestone]
status: planned
---

# 53 - Milestone 11 - UI/UX Uplift

**Status:: ⬜ Planned (spec written 2026-11-06).**  ·  **Phase 4: fuxx looks and feels like a finished tool.**
**Plan:** [[M11 Plan - UI-UX Uplift]] · **Evidence:** [[M11 Input - UI-UX Review]]

## Goal

Make fuxx **fun to live in, without stopping being a professional tool**. Today it works but looks like a prototype:
a grey system titlebar, a terminal touching the window edges, same-looking sidebar rows, a 10 px dot as the only
"an agent needs you" signal, and light themes that fail contrast. After M11 you can tell at a glance which agent
needs you and what it's asking, the window feels designed, and small moments of motion make it feel alive.

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

- **Rows that read at a glance:** 28 px rows; the name on one line (long names end with "…"); the session number in a
  small fixed-width column; the time in its current state on the right ("4m", "32s").
- **Which one is active:** the active session has a filled row and a 2 px accent bar; sessions visible in splits get
  a small marker instead of a fill.
- **Waiting stands out:** a waiting row gets a soft tint of the waiting colour, and the status mark is bigger.
- **"What is it asking?":** while a session waits, the question it's showing (for example "Do you want to make this
  edit to auth.rs?") appears under its name in small text. The row grows only while waiting (AI's call: yes, this is
  on-mission).
- **Group headers** look like headers: smaller caps-style text, a count, the "+" only on hover, and the group's ring
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
- **Dividers** are 1 px and quiet, turning into a 2 px accent line on hover and while dragging.
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

- Every UI colour is derived from the theme with a contrast floor (text at least 4.5:1, marks and lines at least
  3:1), checked by a test over every built-in theme, so light themes (like Catppuccin Latte) read properly.

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

## Success check

1. A dark and a light theme both look designed and readable: titlebar gone, padding, clear active row.
2. With three agents running, one waiting: you can see which one waits, and what it's asking, without clicking.
3. ⇧⌘U takes you to it; ⌘P lists sessions most-urgent first.
4. Working spins, Waiting pulses, Done pops; Animations → Off stops all of it.
5. A split shows pane headers with the focused one marked.
6. Preferences shows theme swatches and a live preview.

## Related

- [[40 - Roadmap]] · [[M11 Input - UI-UX Review]] · [[46b - Milestone 6b - Settings Window]] ·
  [[47 - Milestone 7 - Session Ergonomics]] · [[48 - Milestone 8 - Split Panes]]
