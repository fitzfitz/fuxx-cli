---
tags: [development, milestone]
status: done
---

# 42 - Milestone 2 - Detect the Signal

**Status:: ✅ Done**

## Goal

Spot an OSC notification sequence inside the output stream from Milestone 1.

## Checklist

- [x] Understand the byte shape of OSC 9 (ESC `]` `9` ; ... terminator)
- [x] Scan the captured stream for the ESC byte (`0x1b`) followed by `]9`
- [x] On match, print "NOTIFICATION DETECTED" (don't parse the payload yet)
- [ ] (Later) extract the payload text between the intro and the terminator

**Deferred:** Payload extraction, OSC 99/777 variants, and ConEmu OSC 9 disambiguation are recorded but not yet implemented.

## Success check

Feeding output containing an OSC 9 sequence makes fuxx-cli log a detection.

## What you'll learn

Bytes vs. strings in Rust, pattern-scanning a byte stream, why terminal escape codes
look the way they do. See [[25 - OSC Sequence Detection]].

## Tip

Test with a hand-crafted sequence first (you can `printf` an OSC 9 code in a shell) so
you're not depending on a real agent to trigger detection while developing.

## Related

- [[43 - Milestone 3 - Fire a Notification]] · [[25 - OSC Sequence Detection]]
