---
tags: [architecture, core]
---

# 22 - The Output Stream Pipeline

How data moves through fuxx-cli once the agent is running.

## The flow

```
agent process --> stdout/stderr --> fuxx-cli reads it --> two things happen:
                                        (a) pass it straight through to your screen
                                        (b) scan it for notification sequences
```

The key property: fuxx-cli is a **pass-through observer**. It must not swallow, delay,
or mangle the agent's output — the user should see exactly what they'd normally see.
fuxx-cli reads a copy of the stream on its way past.

## Reading strategy

Output arrives as a stream of bytes, not tidy lines. Notification escape sequences can
appear anywhere. So fuxx-cli reads in chunks and scans the bytes as they flow. Starting
simple (line-by-line) is fine for a first version; byte-level scanning is the robust
version. See [[25 - OSC Sequence Detection]].

## Why bytes matter (a Rust note)

Escape sequences begin with a non-printable byte (`0x1b`, the ESC character). This is
one reason the project touches Rust's distinction between raw bytes (`&[u8]`) and text
(`String`) early. Flagged in [[41 - Milestone 1 - Launch and Capture]].

## Related

- [[21 - The Wrapper Process Model]]
- [[25 - OSC Sequence Detection]]
- [[26 - Notification Dispatch]]
