---
tags: [development, milestone]
status: built
---

# 58 - Milestone 11f - Cursor Style

**Status:: 🟨 Built and reviewed 2026-11-08 (868 tests); release v0.3.2 and the on-screen check pending.**  ·  **Phase 4: an M11 extension.**  ·  **Plan:** [[M11f Plan - Cursor Style]]
**Added to the plan 2026-11-08 by you** (see [[12 - Scope]]). The same plan carries M11e's package icon (see
[[57 - Milestone 11e - Update Available]] "Amendment").

## Goal

The terminal's cursor becomes a thin **bar** that feels alive. It **glides** to where it goes, **breathes** (a soft
glow fading in and out) instead of blinking, and takes the **colour of what the agent in that terminal is doing**.
Two optional **Fun** effects, a comet **trail** and typing **sparks**, are there for those who want more.

## Why now (your words and choices, 2026-11-08)

- From your screenshot of the full block cursor: "I want a blinking cursor, and not a vertical square like this; are
  there other options?" Then: "any cursor options that are fun and entertaining?"
- Offered shapes and effects, you said **"ya pakai cara mu, lebih mantap"** to the suggested mix: **Bar + glide +
  breathing glow + colour following the agent's status**, with **Trail** and **Sparks** as optional "Fun" choices.
- So the mix is yours. Every detail below (timings, sizes, settings names) is AI's call; correct anything before the
  build.

## What exists today

`alacritty_terminal` keeps the cursor shape a program asks for (`ESC [ n q`, e.g. vim's bar in insert mode), and
fuxx draws Block (an inverted cell), Beam (2 px bar), Underline (2 px) and an outline when unfocused. With no program
request, the shape is the terminal's default, a steady block. There is no blinking and no setting.

## How it behaves

### Shape
- **Setting `cursor-style`**: `bar` (default), `underline`, `hollow`, `block`. It becomes the terminal's
  **default** cursor style (`default_cursor_style`). **A program's own request still wins** (vim, Claude Code,
  `ESC [ n q`), and a program's reset (`ESC [ 0 q`) returns to the setting.
- Sizes: bar **2 px** wide, full line height, at the cell's left edge. Underline **2 px** at the bottom. Hollow is a
  **1 px** outline of the cell. Block inverts the cell, as today, so the letter under it stays readable.
- **An unfocused terminal** (another pane, or fuxx in the background) shows a 1 px hollow outline in the theme's
  cursor colour, with no motion, as today.

### Colour follows the agent (`cursor-color`, default `status`)
- The cursor takes the colour of that terminal's status mark, the same colours as the sidebar:
  - Working: the theme's `ring_working`.
  - Waiting for you: `ring_waiting`.
  - Done: `ring_done`.
  - Idle: the theme's own cursor colour.
- `cursor-color = theme` always uses the theme's cursor colour.
- A Block cursor in a status colour draws its letter in the theme's background colour.

### Breathing glow (`cursor-glow`, default on), instead of a hard blink
- The focused cursor's opacity eases between **100%** and **35%** and back, once every **1.6 s** (the waiting
  halo's rhythm), with a soft glow of its own colour around it (**6 px** blur at **30%** opacity).
- **It holds at full strength while you type or the program writes**, and resumes **0.6 s** after the last change, so
  the cursor is always solid where your eyes are.
- It breathes only when a program's style asks for blinking or the program hasn't set one. A program asking for a
  steady cursor gets a steady cursor.
- To spare the battery, the breathing redraws at most **30 times a second**, only for the focused terminal in the
  key fuxx window. It stops when fuxx is in the background.

### Glide (`cursor-glide`, default on)
- When the cursor moves, it slides from its old cell to the new one over **90 ms**, ease-out.
- A move that arrives mid-glide starts a new glide from where the cursor is drawn now, so a burst of output never
  queues up motion.
- A jump of more than **20 rows**, or onto a different screen (alternate screen switch, clear), doesn't glide; it
  appears at once.

### Fun (`cursor-fun`, default off): `trail` or `sparks`
- **Trail:** on a glide longer than one cell, a comet tail of **5** fading copies of the cursor follows it. Each is
  smaller and fainter, and the tail is gone **150 ms** after the cursor stops.
- **Sparks:** each key you type that sends text emits **6** tiny dots (2 px) from the cursor. They fly **6–14 px**
  outward in spread directions, fade over **350 ms**, and are drawn in the cursor's colour. Program output makes no
  sparks, only your typing.

### Motion settings win
With **Animations** off (Preferences) or macOS **Reduce motion** on, there's no glide, glow, trail or sparks: the
cursor is drawn still and solid. The status colour still applies, since it isn't motion.

### Preferences → Appearance → "Cursor"
- **Style:** a choice of Bar / Underline / Hollow / Block.
- **Glide**, **Breathing glow** and **Colour follows the agent:** switches.
- **Fun:** a choice of Off / Trail / Sparks.
- Note under it: "Programs that set their own cursor (vim, for example) still choose their shape."

Settings lines (only non-defaults are written): `cursor-style = underline|hollow|block`, `cursor-glide = off`,
`cursor-glow = off`, `cursor-color = theme`, `cursor-fun = trail|sparks`.

## As built (2026-11-08)

Where the build settled something the plan left open, or changed it:

- **"No program has chosen" is any `HollowBlock`, whatever its blinking flag.** Curses programs (vim, less, htop)
  send `ESC[?12l` when they exit, which would otherwise stop the breathing after each of them. Only a program's
  explicit `ESC [ n q` shape decides steady or breathing.
- **The breathing redraws 20 times a second**, not 30 (still smooth for a 1.6 s breath). Each redraw re-renders
  the window, so a fuxx left idle in front costs less this way.
- **The cursor comes back solid** when its pane gets focus or fuxx becomes the active window again, instead of
  resuming mid-breath.
- **No redraws during the 0.6 s hold**, so an agent printing steadily causes no extra repaints.
- **Clears and resizes don't glide.** A clear is counted from `ESC[2J`, `ESC[3J` and `ESC c` in the output, on both
  the local and the background-service paths. `ESC[H ESC[J` and `ESC[0;2J` still glide (up to 20 rows, 90 ms).
- **A block cursor spans both cells of a wide (CJK) character.**
- **fuxx in the background or another window in front** (Preferences, a popped-out page) shows the still outline,
  so live Style changes from Preferences show once you click back into the terminal.
- **Upgrading with a background service from v0.3.1 or earlier:** a program's steady block in a terminal it owns may
  show your setting until that program sets its cursor again. No crash, no protocol change.
- The Preferences window grew to 698 pt tall for the new rows.

## Not in M11f

- Custom cursor colours or sizes beyond the theme and the status colours; images or emoji as a cursor.
- Shader effects (CRT, bloom on text), sounds, or effects on anything but the cursor.
- The mouse pointer, and the cursor in fuxx's own boxes (rename, filter, address bar).
- A per-terminal cursor setting.

## On-screen checklist (pending)

1. A new terminal shows a thin bar cursor that breathes softly, holding solid while you type.
2. In vim, insert mode shows vim's own shape and normal mode its block; after quitting vim, the bar is back.
3. Run `claude` (or any agent): while it works the cursor is the working colour, waiting for you the waiting
   colour, done the done colour; at an idle prompt it's the theme's colour.
4. Typing and moving with the arrows: the cursor glides cell to cell; `clear` or a full-screen app makes it appear
   at once.
5. Preferences → Cursor → Style Underline / Hollow / Block each changes the cursor right away (no restart).
6. Fun → Trail: moving across a line leaves a short comet tail. Fun → Sparks: typing sprays small dots; program
   output doesn't.
7. Animations off (or macOS Reduce motion): no glide, glow, trail or sparks; the colour still follows the agent.
8. A split pane that isn't focused shows a still outline; switching focus moves the live cursor.
9. Activity Monitor: fuxx idle in the background uses no more CPU than before M11f.
11. Activity Monitor: fuxx idle **in front** at a prompt, Breathing glow on vs off. Note the CPU of each; if the
    difference is noticeable, say so (the breath rate can go lower).
12. In Claude Code: if it hides the terminal's cursor and draws its own, the status colour shows only outside it
    (at the shell prompt, or in other agents). Note what you see.
10. Light and dark themes: every shape and colour is visible (the ui-shots cover this too).
