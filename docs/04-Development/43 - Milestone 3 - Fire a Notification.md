---
tags: [development, milestone]
status: not-started
---

# 43 - Milestone 3 - Fire a Notification

**Status:: ⬜ Not started**  ·  **This completes v1.**

## Goal

When Milestone 2 detects a sequence, show a real macOS desktop notification.

## Checklist

- [ ] Add `notify-rust` as a dependency (`cargo add notify-rust`)
- [ ] Fire a test notification on startup to prove the crate works
- [ ] Wire detection → dispatch: on OSC detection, fire the notification
- [ ] Keep detection and dispatch as separate functions/modules

## Success check

Run a real agent through fuxx-cli; when it finishes, macOS shows a banner. **This is a
complete, useful tool. Stop adding features here.**

## What you'll learn

Adding/using a crate, keeping modules decoupled, end-to-end wiring.

## Related

- [[26 - Notification Dispatch]] · [[44 - Milestone 4 - Ship via Homebrew]]
