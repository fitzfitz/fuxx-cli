---
tags: [tech, decision]
---

# 31 - Rust Language Choice

Recorded decision: fuxx-cli is written in Rust. This was a firm choice by the author.

## Why Rust works here

- Compiles to a single native binary — ideal for a CLI, trivial to distribute.
- Excellent CLI ecosystem (`clap`, `ratatui`, etc.).
- Strong correctness guarantees once it compiles.

## The honest tradeoff

Rust is the steeper learning curve for a first shipped tool. The concepts that will bite
earliest in *this* project:

- **Ownership & borrowing** — Rust's central concept; shows up as soon as you pass data
  around between the reader and the detector.
- **Bytes vs. strings** (`&[u8]` vs `String`) — unavoidable because escape sequences are
  raw bytes. See [[22 - The Output Stream Pipeline]].
- **Error handling** (`Result`, the `?` operator) — spawning processes and doing I/O can
  fail, and Rust makes you handle it.

## Attitude to set

Getting stuck on these is the learning, not a failure signal. Expect friction in
[[41 - Milestone 1 - Launch and Capture]] and [[42 - Milestone 2 - Detect the Signal]].

## Related

- [[32 - Cargo and Project Layout]]
- [[50 - Agentic Workflow]]
