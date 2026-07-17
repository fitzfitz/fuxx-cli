---
tags: [development, milestone]
status: not-started
---

# 41 - Milestone 1 - One Terminal Pane

**Status:: ⬜ Not started**  ·  **The make-or-break milestone.**

## Goal

Render **one working terminal** inside a GPUI window: `alacritty_terminal` (grid + VT parser) fed by
`portable-pty` (a login shell), drawn by a GPUI `TerminalView`, with keyboard input routed back to the
PTY. Proves the whole rendering approach.

## Rough scope / checklist

- [ ] Spawn a shell on a PTY (`portable-pty`, reused from v1); pump its bytes into an
      `alacritty_terminal` `Term`.
- [ ] A GPUI view that reads the `Term` grid and renders cells (text, colors, cursor) — glyphs via
      `cosmic-text`/`glyphon` or GPUI text (decide + verify during the milestone).
- [ ] Route keyboard input from the GPUI view to the PTY.
- [ ] Handle resize (window → PTY size → `Term` resize).
- [ ] Redraw on new output.

## Success check

`cargo run` opens a window with a usable shell: run `ls --color`, `vim`, type and see output render
correctly; resizing works. **You verify visually** (GUI can't be checked by the AI).

## Notes

- This is where most of the work/risk lives (rendering an `alacritty_terminal` grid in GPUI). If it
  proves too costly, the fallback is `gpui-ghostty` (needs Zig) — see [[30 - Tech Stack]].

## Related

- [[42 - Milestone 2 - Agent State Detection]] · [[20 - Architecture Overview]]
