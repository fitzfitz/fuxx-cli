---
tags: [development, milestone]
status: done
---

# 42 - Milestone 2 - Agent State Detection

**Status:: ✅ Done (2026-10-26). Status changes logged and confirmed on screen, incl. a real `claude` run.** D4 = `Idle` status used; constants not tuned (150 ms / 3 s / 8 lines).
**Plan:** [[M2 Plan - Agent State Detection]] (step-by-step, compile-verified, with status per task)

## Goal

For a session running an agent, detect its **state** — working / done / waiting-for-input — from the
output stream. This drives the rings in [[44 - Milestone 4 - Notification Rings]].

## Rough scope / checklist

- [x] Reuse v1's **OSC-9 detection** (the `OscDetector` logic, in git history under
      [[90-Archive-v1-CLI-Wrapper]]) against the session's byte stream, **extended (plan change
      2026-10-26)** to OSC 99 (kitty) and OSC 777 `notify` (Ghostty), skipping the `OSC 9;4`
      progress bar; plus a real BEL (Terminal.app) from the parser.
- [x] Add heuristics for **waiting-for-input** (e.g. output idle after a prompt) and **working**
      (recent output) vs **done** (a notification / idle).
- [x] Expose a per-session `Status` the UI can read: reported to the view on every change
      (`TerminalView::status_changed`); M4 keeps the latest value per session.

## Success check

Run an agent in the pane; as it works, finishes, and waits for input, the detected `Status` changes
correctly (verify via a debug indicator or log).

## Related

- [[43 - Milestone 3 - Many Sessions]] · [[11 - Features]] (section C)
