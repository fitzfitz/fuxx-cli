---
tags: [development, milestone]
status: planned
---

# 55 - Milestone 11c - Native Notifications

**Status:: ⬜ Planned 2026-11-06 (your go).**  ·  **Phase 4: an M11 extension.**  ·  **Plan:** [[M11c Plan - Native Notifications]]
**Moved from M12:** "notifications named fuxx" and "clicking a notification switches to that session" (D5, 2026-10-26),
because the test below showed neither needs the paid Apple signature.

## Goal

fuxx's desktop notifications come from **fuxx** itself — titled fuxx, listed under fuxx in System Settings →
Notifications — and **clicking one takes you to the terminal that raised it**: fuxx comes forward on that workspace,
pane and tab.

## Why now (your choices, 2026-11-06)

- "Extend M11" with the notification part of M12, once a test proved it works without the paid Apple account.
- The test, on your Mac, with the ad-hoc signed `target/dist/fuxx.app` (bundle `io.github.fitzfitz.fuxx`), through
  Apple's UserNotifications framework (`UNUserNotificationCenter`, as Ghostty uses), throwaway branch deleted:
  1. Opened as an app: macOS granted permission, the banner read **fuxx**, and **the click came back to fuxx** (on the
     main thread; fuxx answered with a second banner). Screenshot confirmed by you.
  2. As a background process started from the bundle's binary (how the background service runs): **before** fuxx was
     allowed, macOS refused without asking ("Notifications are not allowed for this application"); **after**, its
     banner showed, titled fuxx. Screenshot confirmed by you.
  3. Not run from the bundle (`cargo run`): there is no app identity; the framework can't be used.

## How it behaves

- **The banners.** Same moments and wording as today (an agent finishes, an agent waits for input, `fuxx notify`,
  the background service while the window is closed), same "the terminal you're looking at doesn't nag" rule. The
  title is today's summary, the body today's text; the banner says fuxx and carries which terminal raised it. No sound
  (as today).
- **Clicking a banner** brings fuxx forward and shows that terminal: selects its workspace, focuses its pane, selects
  its tab. If that terminal has closed since, fuxx just comes forward.
- **With the window closed** (background service on): clicking a banner opens fuxx; once fuxx has reattached to the
  service and restored the layout, it shows that terminal.
- **Permission.** The window asks macOS once, at launch (macOS shows its "Allow notifications from fuxx?" question
  only the first time; later launches get the stored answer without asking). AI's call: at launch rather than at the
  first notification, because the background service can't ask, and the first notification may come from it.
- **Fallback: never silent.** Whenever macOS won't take fuxx's banner — you haven't answered yet, you chose "Don't
  Allow", you switched fuxx off in System Settings later (macOS reports these two the same way), or fuxx isn't
  running from an app bundle (a `cargo run` build) — fuxx sends today's `osascript` banner instead (titled "Script
  Editor", no click-through). AI's call: an agent waiting for you should never go unnoticed; fuxx has no on/off switch
  of its own today, so silencing all banners means switching off Script Editor's too (see Not in M11c).

## Settled before planning (AI's calls, after a builder's read)

1. **A click on a banner the background service posted is untested** (the test's banner showed, but nobody clicked
   it). Which process macOS hands that click to — the running service, a newly launched fuxx window, or nothing — is
   unknown. **The plan starts with a short test of exactly this** (with the service running and the window closed,
   and with the window open). If the click reaches the service, the service passes it to the window (opening
   `fuxx.app` when none is attached) over its existing connection; if it opens a fresh fuxx, the window takes it
   directly. The result decides the plan's shape before anything else is built.
2. **"That terminal" is its session id** — the `sN` that `fuxx ls` shows, which the background service keeps across
   window reattaches (M10a). The banner carries it; the window finds the tab holding `sN` in its layout.
3. **A click before the layout exists** (fuxx opened by the click) is held as one pending target and shown once
   reattaching and restoring have finished; a later click replaces it. The same path serves a click while fuxx runs.
4. **"Running from the app bundle"** means the main bundle's identifier is `io.github.fitzfitz.fuxx`; anything else
   (a `cargo run` build) uses the old banners.
5. **The fallback is decided per notification**, in the window and in the service alike: fuxx reads macOS's current
   answer for fuxx; "allowed" → a fuxx banner, anything else → the old banner. If macOS rejects a banner anyway, the
   old one is sent instead. The read happens off the UI thread, as `fire` does today.
6. **New dependency: `objc2-user-notifications` 0.3.2** (the same family as fuxx's other Apple bindings, at the version
   the test used; you agreed to it before the test). GPUI's own app delegate is left alone: notifications get their
   own delegate object.
7. **Every banner is new** (no replacing an earlier one). A banner click leaves the 🔔 list's unseen count alone: that
   count means "new since you last opened the list" and isn't kept per terminal (corrected while planning; the
   earlier "marks that terminal's entries seen" didn't fit the list).

## Not in M11c

- An app icon (banners show a blank icon until fuxx has one — separate work, not planned yet).
- Developer ID signing and notarization, Gatekeeper's "Open Anyway" (still M12).
- Notification buttons, Dock badges, grouping, sounds, removing old banners from Notification Center.
- A fuxx setting to turn notifications off (none exists today).

## Success check

On screen (you): a finished agent's banner reads fuxx; clicking it lands on its tab — from another tab, another
workspace, with fuxx in the background, and with the window closed and the service on. Fresh permission (reset via
`tccutil reset All io.github.fitzfitz.fuxx`, AI's suggestion): the question appears at launch; "Don't Allow" → the
old banners still come; allowing it later in System Settings → fuxx banners from then on. `cargo run` still notifies
(old banners).

## Related

[[40 - Roadmap]] · [[44 - Milestone 4 - Notification Rings]] (D5) · [[50 - Milestone 10a - Persistent Sessions]] ·
[[53 - Milestone 11 - UI-UX Uplift]]
