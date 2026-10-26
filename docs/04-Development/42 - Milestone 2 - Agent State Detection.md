---
tags: [development, milestone]
status: not-started
---

# 42 - Milestone 2 - Agent State Detection

**Status:: ⬜ Not started**
**Plan:** [[M2 Plan - Agent State Detection]] (step-by-step, compile-verified, with status per task)

## Goal

For a session running an agent, detect its **state** — working / done / waiting-for-input — from the
output stream. This drives the rings in [[44 - Milestone 4 - Notification Rings]].

## Rough scope / checklist

- [ ] Reuse v1's **OSC-9 detection** (the `OscDetector` logic, in git history under
      [[90-Archive-v1-CLI-Wrapper]]) against the session's byte stream, **extended (plan change
      2026-10-26)** to OSC 99 (kitty) and OSC 777 `notify` (Ghostty), skipping the `OSC 9;4`
      progress bar; plus a real BEL (Terminal.app) from the parser.
- [ ] Add heuristics for **waiting-for-input** (e.g. output idle after a prompt) and **working**
      (recent output) vs **done** (a notification / idle).
- [ ] Expose a per-session `Status` the UI can read.

## Success check

Run an agent in the pane; as it works, finishes, and waits for input, the detected `Status` changes
correctly (verify via a debug indicator or log).

## Related

- [[43 - Milestone 3 - Many Sessions]] · [[11 - Features]] (section C)
