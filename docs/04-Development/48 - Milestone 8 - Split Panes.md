---
tags: [development, milestone]
status: built-awaiting-check
---

# 48 - Milestone 8 - Split Panes

**Status:: 👀 Built, reviewed and merged to `master` 2026-10-27; your on-screen check is still to do.**  ·  **Phase 2 (moved up from Phase 3 on request).**
**Plan:** [[M8 Plan - Split Panes]] (step-by-step, with status per task)

## Goal

See several sessions at once. Split the terminal area into panes, and split each pane again, like
Ghostty or iTerm. Every pane is a normal session with its own ring in the sidebar. ([[40 - Roadmap]]
M8; [[11 - Features]] D.)

## Your choices (2026-10-27)

- Split panes are now in scope, and M8 is built right after M7, before your M6b check.
- **Nested splits like Ghostty/iTerm:**
  - ⌘D splits right and ⌘⇧D splits down.
  - Any pane can be split again.
  - Drag a divider to resize.
  - Each pane is its own session with its own ring in the sidebar.

Everything else below is the AI's call. Say if any should change.

## How it behaves

### Splitting

- **⌘D (Split Right)** and **⌘⇧D (Split Down)** split the pane you're in. The new pane opens beside it
  (right) or under it (down), with a **new session** in the same group as the pane you split (like
  ⌘T). The new pane gets the focus.
- The pane you split and the new one share the space **half and half**. Splitting again works on any pane,
  so layouts nest (e.g. one tall pane on the left, two stacked on the right).
- These are also in a new **Pane** menu.

### Moving between panes

- **Click** a pane to focus it.
- **⌥⌘← / → / ↑ / ↓** go to the pane in that direction.
- **⌘[ / ⌘]** go to the previous / next pane in the layout, in reading order.
- The **focused pane** shows the cursor as usual. The others are slightly dimmed, as in Ghostty.

### Resizing

- **Drag the line between two panes** to resize them. The pointer turns into a resize arrow over it.
  A pane can't be dragged smaller than 10% of the split.
- **Pane → Equalize Panes (⌃⌘=)** makes all panes in the layout share the space evenly.
- Each pane's program is told its new size, as when you resize the window.

### The sidebar and sessions

- Every pane is a session in the sidebar, with its own ring, name and group. Splitting adds a session.
  It goes where ⌘T would put it: at the end of its group.
- **Which panes show:** the layout holding the session you select. Clicking a session in the sidebar
  shows its layout and focuses that pane. Sessions that are **on screen but not focused** get a faint
  highlight in the sidebar, and the focused one gets the normal one.
- A session that was never split is a layout of its own, as today.
- **Closing** a pane (⌘W, or the program exits) closes its session. Its neighbour takes the space and
  gets the focus. Closing the last session still quits fuxx.
- Notifications are unchanged: only the focused session counts as "the one you're looking at".
- Renaming, reordering, groups and presets (M7) work the same. Reordering only changes the sidebar, not
  the panes.

### Restore

- Session restore (M7) also brings back **which sessions share a layout, how they're split and the
  divider positions**.
- If the saved layout doesn't match the saved sessions (for example a hand-edited file), the
  mismatched sessions come back on their own, and fuxx logs a line.

## Rough scope / checklist

- [ ] Split Right (⌘D) / Split Down (⌘⇧D), nested, new session in the same group.
- [ ] Focus by click, ⌥⌘arrows, ⌘[ / ⌘]; unfocused panes dimmed.
- [ ] Drag dividers (10%–90%); Equalize Panes (⌃⌘=); programs get the new size.
- [ ] Sidebar shows which sessions are on screen; selecting a session shows its layout.
- [ ] Closing a pane gives its space to the neighbour; restore brings layouts back.

## Not in M8

- Moving a pane to another layout or dragging panes around.
- Zooming one pane to fill the area.
- Different groups' sessions in one layout are allowed (a split takes the group of the pane you split), but there's no "split into another folder" command.
- Tabs, or more than one window.
- Pane-specific settings (font, theme).

## Success check

Press ⌘D, then ⌘⇧D in the new pane: three panes, one on the left and two stacked on the right. Each
pane runs its own shell, and `tput cols` matches its width. Drag the vertical divider: the panes
resize and `tput cols` changes. ⌥⌘← goes back to the left pane. Quit and reopen: the same three panes
come back with the same divider positions.

## Related

- [[40 - Roadmap]] · [[11 - Features]] · [[12 - Scope]] · [[47 - Milestone 7 - Session Ergonomics]] · [[00 - Plan Tracker (Start Here)]]
