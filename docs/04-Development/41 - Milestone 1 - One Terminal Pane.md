---
tags: [development, milestone]
status: done
---

# 41 - Milestone 1 - One Terminal Pane

**Status:: ✅ Done (2026-10-26). A usable shell renders in the GPUI window; confirmed on screen**  ·  **The make-or-break milestone.**
**Plan:** [[M1 Plan - One Terminal Pane]] (step-by-step, compile-verified, with status per task)

## Goal

Render **one working terminal** inside a GPUI window: `alacritty_terminal` (grid + VT parser) fed by
`portable-pty` (a login shell), drawn by a GPUI `TerminalView`, with keyboard input routed back to the
PTY. Proves the whole rendering approach.

## Rough scope / checklist

- [x] Spawn a shell on a PTY (`portable-pty`, reused from v1); pump its bytes into an
      `alacritty_terminal` `Term`.
- [x] A GPUI view that reads the `Term` grid and renders cells (text, colors, cursor) — glyphs via
      `cosmic-text`/`glyphon` or GPUI text (decide + verify during the milestone). **Decided (D2,
      2026-10-26): GPUI's own text system** (`shape_line` + `paint`), like Zed's terminal.
- [x] Route keyboard input from the GPUI view to the PTY.
- [x] Handle resize (window → PTY size → `Term` resize).
- [x] Redraw on new output.

## Success check

`cargo run` opens a window with a usable shell: run `ls --color`, `vim`, type and see output render
correctly; resizing works. **You verify visually** (GUI can't be checked by the AI).

## Notes

- This is where most of the work/risk lives (rendering an `alacritty_terminal` grid in GPUI). If it
  proves too costly, the fallback is `gpui-ghostty` (needs Zig) — see [[30 - Tech Stack]].

## Related

- [[42 - Milestone 2 - Agent State Detection]] · [[20 - Architecture Overview]]
