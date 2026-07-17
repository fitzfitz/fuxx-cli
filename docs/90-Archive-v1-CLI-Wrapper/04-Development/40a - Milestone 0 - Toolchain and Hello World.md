---
tags: [development, milestone]
status: done
---

# 40a - Milestone 0 - Toolchain and Hello World

**Status:: ✅ Done**

## Goal

Confirm the full edit → compile → run loop works before writing anything real.

## Checklist

- [x] Install Rust via `rustup` (rustc/cargo 1.97.1, stable, aarch64-apple-darwin)
- [x] `cargo new fuxx-cli` (used `cargo init --name fuxx-cli --bin` at the repo root, so
      the crate lives alongside `docs/`)
- [x] Make it print something (`src/main.rs` prints the crate version via `env!`)
- [x] `cargo run` shows the output → `fuxx-cli v0.1.0 — toolchain works.`
- [x] Open the project; understand what `Cargo.toml` and `src/main.rs` are

## Success check

Running `cargo run` prints your message with no errors.

## What you'll learn

Basic Cargo usage ([[32 - Cargo and Project Layout]]), project layout, the compile loop.

## AI delegation note

Safe to delegate scaffolding, but read every generated line and be able to explain it.
See [[51 - Delegation Principles]].

## Related

- [[40 - Development Roadmap]] · [[41 - Milestone 1 - Launch and Capture]]
