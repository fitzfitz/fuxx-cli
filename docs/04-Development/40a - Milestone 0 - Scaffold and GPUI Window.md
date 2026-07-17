---
tags: [development, milestone]
status: not-started
---

# 40a - Milestone 0 - Scaffold and GPUI Window

**Status:: ⬜ Not started**

## Goal

Repurpose the crate and get a **blank GPUI window** to launch — prove the GPUI toolchain builds and
runs before adding the terminal core.

## Checklist

- [ ] Decide the crate layout in this (repurposed) repo — keep v1 source or start the app crate fresh
      (v1 code is preserved in git history regardless)
- [ ] Add GPUI as a **git dependency pinned to a Zed commit** (verify a known-good rev live)
- [ ] Minimal app: `Application::new().run(...)` → `open_window()` → a root view showing a placeholder
- [ ] `cargo run` opens a window (you confirm visually — GUI can't be verified by the AI)

## Success check

`cargo run` opens a GPUI window on screen with a placeholder view; closing it exits cleanly.

## Notes / risks

- **GPUI first build is heavy** (large dependency tree from the Zed repo) and pre-1.0 (pin the rev).
- Needs stable Rust (have 1.97.1) + macOS/Linux.
- Verification is **hands-on-yours** (build & run, screenshot) — like v1's `vim`/banner checks.

## Related

- [[41 - Milestone 1 - One Terminal Pane]] · [[30 - Tech Stack]]
