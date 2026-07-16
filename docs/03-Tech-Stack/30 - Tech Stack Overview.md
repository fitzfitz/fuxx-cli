---
tags: [moc, tech]
---

# 30 - Tech Stack Overview

The map of every technology fuxx-cli uses and why.

## The stack at a glance

| Layer | Choice | Note |
|---|---|---|
| Language | Rust | [[31 - Rust Language Choice]] |
| Build/pkg manager | Cargo | [[32 - Cargo and Project Layout]] |
| Argument parsing | `clap` | [[33 - Key Crates]] |
| Notifications | `notify-rust` | [[33 - Key Crates]] |
| Process/PTY | std + possibly a PTY crate | [[33 - Key Crates]] |
| Distribution | Homebrew tap | [[34 - Homebrew Distribution]] |
| Version control | Git + GitHub | [[35 - Repo and Git]] |
| Target OS (v1) | macOS | [[12 - Scope Boundaries]] |

## Reading order

Start with [[31 - Rust Language Choice]], then [[32 - Cargo and Project Layout]] to
understand how a Rust project is built, then [[33 - Key Crates]] for the specific
libraries, then [[34 - Homebrew Distribution]] for shipping.

## Related

- [[20 - Architecture Overview]]
- [[40 - Development Roadmap]]
