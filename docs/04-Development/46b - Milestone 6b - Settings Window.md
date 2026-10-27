---
tags: [development, milestone]
status: not-started
---

# 46b - Milestone 6b - Settings Window

**Status:: ⬜ Not started (plan written 2026-10-27).**  ·  **Phase 2: change how fuxx looks without editing a file.**
**Plan:** [[M6b Plan - Settings Window]] (step-by-step, with status per task)

## Goal

**File → Preferences…** (or **cmd-,**) opens a small window. In it you pick the **theme**, the **font**,
the **size** and whether **ligatures** are on. Every click applies at once to every session, and is saved
to `~/.config/fuxx/config`, so it is still there next time. Added on request 2026-10-27 ([[40 - Roadmap]] M6b).

## How it behaves (the calls made for this plan, 2026-10-27)

You said "go", so these are the AI's calls unless marked as yours. Say if any should change.

- **Menus.** fuxx gets a Mac menu bar. It had none before.
  - **fuxx** menu: Quit fuxx (cmd-Q).
  - **File** menu: New Session (cmd-T), Close Session (cmd-W), then **Preferences… (cmd-,)**. This is your choice: File → Preferences and cmd-,.
- **The window.**
  - It is a fixed-size window titled "Preferences".
  - If it's already open, cmd-, brings it to the front.
  - cmd-W or the red button closes it.
  - Closing the terminal window still quits fuxx, even with Preferences open.
- **What's in it**, top to bottom:
  - **Theme**: a scrolling list. "Default" first, then the built-in themes and the ones in your `~/.config/fuxx/themes`, alphabetically. A theme in your folder replaces a built-in with the same name. The current one is highlighted.
  - **Font**: a scrolling list of your installed **fixed-width** fonts, with "Default" first (JetBrains Mono if installed, else Menlo). The current one is highlighted.
  - **Size**: − and + buttons around the number, in steps of 1 point, within 4–200.
  - **Ligatures**: an On/Off switch.
  - **D9, your choice:** no typing. Colors are chosen through the theme only; there are no hex boxes and no per-color controls.
- **Clicking applies and saves at once.** There is no Save button, as is usual in Mac preferences.
  - The open sessions, the sidebar and new sessions all switch.
  - A font or size change re-fits the grid, so programs get the new size.
- **How saving works.**
  - fuxx changes only the line for that setting:
    - Theme → `theme = Name`.
    - Font → `font-family = Name`.
    - Size → `font-size = N`.
    - Ligatures off → `font-feature = -calt`; back on → that line is removed.
  - Choosing "Default" removes the line.
  - Everything else in the file stays exactly as you wrote it: comments, your color overrides, includes.
  - If the file doesn't exist yet, it is created. It is written safely: a temporary file, then renamed over the old one.
  - This changes the M6 rule "fuxx never writes settings files". Now only the Preferences window writes, and only `~/.config/fuxx/config`, never an included file.
- **Your own lines still win.** For example, a `background = #000000` you added keeps applying after you switch themes, as in M6.

## Also in M6b: the default session name (added on request 2026-10-27)

- A new session's default name changes from "Shell N" to **🖕**. The sidebar row still starts with its
  number (`2  🖕`), so sessions stay easy to tell apart. The window title and notifications use the same name.
- A program that sets its own title (for example Claude Code) still shows that title instead.
- Renaming sessions yourself is still M7.

## Rough scope / checklist

- [ ] Menu bar: fuxx (Quit), File (New/Close Session, Preferences… cmd-,).
- [ ] Preferences window: opens or comes to the front; closes with cmd-W.
- [ ] Theme list, font list (fixed-width only), size −/+, ligatures switch; current values highlighted.
- [ ] Each change applies live to all sessions and the sidebar, and is saved to the settings file (only that line changes).
- [ ] Default session name is 🖕 instead of "Shell N".

## Not in M6b

- **Typing:** no hex colors or font names (D9).
- **No per-color pickers.**
- **No preview** of a theme before you pick it.
- **No other settings** beyond theme, font, size and ligatures.
- **Hand edits don't apply live:** editing the file by hand still needs a restart (no file watching).
- **No About window.**
- **No Edit or Window menus.**

## Success check

cmd-, opens Preferences. Clicking "Catppuccin Latte" turns every session and the sidebar light at once. Picking
a different font and pressing + resizes the text live. After quitting and reopening fuxx, the same choices are
still there, and your other lines in `~/.config/fuxx/config` are untouched.

## Related

- [[46 - Milestone 6 - Fonts and Config]] · [[40 - Roadmap]] · [[12 - Scope]] · [[00 - Plan Tracker (Start Here)]]
