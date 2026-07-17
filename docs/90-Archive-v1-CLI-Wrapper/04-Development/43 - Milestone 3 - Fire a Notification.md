---
tags: [development, milestone]
status: done
---

# 43 - Milestone 3 - Fire a Notification

**Status:: ✅ Done**  ·  **This completes v1.**

## Goal

When Milestone 2 detects a sequence, show a real macOS desktop notification.

## Checklist

- [x] ~~Add `notify-rust`~~ → **dropped for `osascript`** (notify-rust's deprecated
      `NSUserNotification` backend doesn't display on modern macOS; see [[33 - Key Crates]])
- [x] Prove the notifier works — build/link proof, then confirmed a real banner shows
- [x] Wire detection → dispatch: on OSC detection, fire the notification
- [x] Keep detection and dispatch as separate functions/modules (`detector` vs `notifier`)

## Success check

Run a real agent through fuxx-cli; when it finishes, macOS shows a banner. **This is a
complete, useful tool. Stop adding features here.** ✅ Verified: a detection fires a visible
banner via `osascript` (attributed to "Script Editor" — fine for v1).

## What you'll learn

Adding/using a crate, keeping modules decoupled, end-to-end wiring.

## Related

- [[26 - Notification Dispatch]] · [[44 - Milestone 4 - Ship via Homebrew]]
