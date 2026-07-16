---
tags: [development, milestone]
status: not-started
---

# 41 - Milestone 1 - Launch and Capture

**Status:: ⬜ Not started**

## Goal

Launch another program from fuxx-cli and read its output. This is the heart of the tool.

## Checklist

- [ ] Parse the command + args after `fuxx` (start with raw args; add `clap` when ready)
- [ ] Spawn that command as a child process (`std::process::Command`)
- [ ] Read the child's stdout stream in your code
- [ ] Pass the output through so the user still sees it
- [ ] Exit with the child's exit code

## Success check

`cargo run -- echo hello` makes fuxx-cli print back what the child printed.

## What you'll learn

`std::process::Command`, reading streams, ownership/borrowing when passing the stream
around, `Result`/`?` error handling. This is where Rust's curve bites — expect it.

## Known risk

The agent may disable colors/interactivity if it doesn't detect a real terminal. If so,
this milestone expands to include a PTY. See [[21 - The Wrapper Process Model]] and
[[52 - Known Risks and Gotchas]].

## Related

- [[42 - Milestone 2 - Detect the Signal]] · [[22 - The Output Stream Pipeline]]
