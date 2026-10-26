---
tags: [development, milestone]
status: not-started
---

# 45 - Milestone 5 - Terminal UX

**Status:: ⬜ Not started (plan written 2026-10-26).**  ·  **Phase 2: a terminal you can live in.**
**Plan:** [[M5 Plan - Terminal UX]] (step-by-step, with status per task)

## Goal

Make fuxx's terminal pane behave like a normal Mac terminal: scroll back through output, select and
copy text, paste, let programs that use the mouse (vim, htop, full-screen TUIs) get it, open links,
and keep everything correct when the window is resized ([[40 - Roadmap]], M5 line; [[11 - Features]] A).

## How it behaves (the calls made for this plan, 2026-10-26)

You said "go" without questions, so these are the AI's calls, modelled on Zed's terminal (same GPUI
and `alacritty_terminal`). Say if any should change.

- **Scrollback:** 10,000 lines per session (alacritty's default). Trackpad/mouse wheel scrolls it;
  **shift-PageUp / shift-PageDown** and **cmd-↑ / cmd-↓** move a page. New output while you're
  scrolled up **doesn't** yank you down; **typing or pasting** jumps back to the bottom.
- **Selection:** drag to select; **double-click** a word, **triple-click** a line; **shift-click**
  extends. Selected cells get a blue background. Typing clears the selection.
- **Copy / paste:** **cmd-c** copies the selection (it stays selected); **cmd-v** pastes. Programs
  that ask for "bracketed paste" (zsh, Claude Code, vim) get it marked as a paste, with escape
  characters removed; otherwise line breaks are sent as Enter.
- **Programs that use the mouse** (they switch "mouse reporting" on): clicks, drags and the wheel go
  to the program. Hold **shift** to select text locally instead. In full-screen programs without
  mouse reporting (less, man), the wheel sends ↑/↓.
- **Links:** hold **cmd** and hover a link → it's underlined and the pointer becomes a hand;
  **cmd-click** opens it in your browser. Works for plain-text URLs and for links programs mark up
  (OSC 8). **Only `http://`, `https://` and `mailto:`** open, so program output can't make fuxx launch
  files or other apps.
- **Resize:** programs are told the real pixel size of the terminal (not 0), and only when the cell
  count or cell size changes. A width change clears the selection (the text reflows).
- **Agent status keeps working while you're scrolled up:** the "waiting for you" check reads the
  live screen, not the part of the history you're looking at.

## Rough scope / checklist

- [ ] Scrollback with wheel, page keys, snap-to-bottom on typing.
- [ ] Selection (drag, word, line, shift-extend), copy, paste (bracketed when asked).
- [ ] Mouse reporting for programs that request it; shift bypass; wheel → arrows in full-screen programs.
- [ ] cmd-hover / cmd-click links (http, https, mailto only).
- [ ] Real pixel sizes on resize.
- [ ] Status detection unaffected by scrolling.

## Not in M5

Search in scrollback (P2), clickable file paths ([[11 - Features]] lists "URL / path"; the roadmap
says URLs, so paths wait), copy-on-select, auto-scroll while dragging a selection past the edge,
right-click menu, fonts/prompt icons (M6).

## Success check

In a real session: scroll back through a long `claude` run, select and copy part of it, paste it
into another session, cmd-click a link in it, use the mouse in `vim` (`:set mouse=a`) and `htop`,
and resize the window without garbled output.

## Related

- [[40 - Roadmap]] · [[11 - Features]] · [[12 - Scope]] · [[00 - Plan Tracker (Start Here)]]
