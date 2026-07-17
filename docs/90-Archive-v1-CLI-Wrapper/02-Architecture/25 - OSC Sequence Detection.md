---
tags: [architecture, core]
---

# 25 - OSC Sequence Detection

The "brain" of fuxx-cli: recognizing when the agent is signaling for attention.

## What an OSC sequence is

OSC = "Operating System Command." It's a category of terminal escape sequence that
programs emit to talk to the terminal (set the window title, send a notification, etc).
They begin with the ESC byte (`0x1b`) followed by `]`, then a number identifying the
command, then a payload, then a terminator.

## The ones fuxx-cli cares about

- **OSC 9** — a simple desktop-notification sequence.
- **OSC 777** — a notification format (`notify;title;body`) used by some terminals.
- **OSC 99** — a newer structured notification protocol.

AI coding agents emit these to say things like "task complete" or "waiting for input."
fuxx-cli's job is to spot them in the output stream.

## Detection strategy (staged)

1. **v0 (prove it):** scan for the ESC byte followed by `]9` and just log "detected."
2. **v1 (useful):** properly find the start and terminator, extract the payload text.
3. **later (robust):** handle all three sequence types and malformed input gracefully.

Don't build the robust version first. Prove detection works, then improve it.

## Related

- [[22 - The Output Stream Pipeline]]
- [[26 - Notification Dispatch]]
- [[42 - Milestone 2 - Detect the Signal]]
