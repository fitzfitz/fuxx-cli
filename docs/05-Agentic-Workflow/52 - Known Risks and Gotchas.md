---
tags: [workflow, risks]
---

# 52 - Known Risks and Gotchas

Things likely to trip up the project, recorded so they don't surprise you (or an AI).

## Technical

- **TTY vs pipe:** the wrapped agent may disable colors/interactivity when its output is
  a pipe rather than a real terminal. Fix: a PTY. See [[21 - The Wrapper Process Model]].
- **Bytes vs strings:** escape sequences are raw bytes; treating output as tidy UTF-8
  text will bite. See [[22 - The Output Stream Pipeline]].
- **Sequence variants:** OSC 9 / 99 / 777 differ in format. Start with one; don't try to
  parse all three at once. See [[25 - OSC Sequence Detection]].
- **Crate versions drift:** verify current versions on crates.io; don't trust an AI's
  remembered version numbers. See [[33 - Key Crates]].
- **macOS notifications from a CLI binary (discovered in M3):** `notify-rust` (via
  `mac-notification-sys`) uses the **deprecated `NSUserNotification` API, which silently does
  not display** on modern macOS (26.x) for an unbundled binary — `show()` returns `Ok` but no
  banner appears, and no bundle-id attribution helps. fuxx uses **`osascript`
  (`display notification`)** instead, which works. Symptom to remember: "notification call
  succeeds but nothing shows" → it's the deprecated API, not permissions. See [[33 - Key Crates]].

## Process

- **Scope creep** toward a full terminal / dashboard. Guard with [[12 - Scope Boundaries]].
- **Lost progress** between sessions if the [[60 - Progress Log]] isn't updated.
- **Accepting code you don't understand** — see [[51 - Delegation Principles]].

## External

- **The GitHub repo returned 404** at vault creation. Create/repair it before pushing.
- **Homebrew tap** is fiddly the first time; treat it as its own milestone
  ([[44 - Milestone 4 - Ship via Homebrew]]).

## Related

- [[50 - Agentic Workflow]] · [[40 - Development Roadmap]]
