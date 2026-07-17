---
tags: [architecture, core]
---

# 26 - Notification Dispatch

The output side: turning a detection into something the user actually perceives.

## What happens on a detection

When [[25 - OSC Sequence Detection]] finds a notification sequence, fuxx-cli triggers a
**native macOS desktop notification** (a banner). Optionally it can also ring the
terminal bell.

## How (tech)

Via the `notify-rust` crate, which wraps the OS notification system. See
[[33 - Key Crates]]. The dispatch code should be small and isolated so it's easy to test
in isolation ("fire a fake notification on startup to prove it works").

## Design note

Keep dispatch decoupled from detection. Detection decides *that* something happened;
dispatch decides *how the user is told*. Keeping them separate makes each easy to change
later (e.g. swapping desktop banners for a sound, or adding the agent's name).

## Related

- [[25 - OSC Sequence Detection]]
- [[43 - Milestone 3 - Fire a Notification]]
- [[33 - Key Crates]]
