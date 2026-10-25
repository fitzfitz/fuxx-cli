---
tags: [development, milestone]
status: in-progress
---

# 40a - Milestone 0 - Scaffold and GPUI Window

**Status:: 🟨 In progress. Was blocked on Xcode; a way to build without it was found (`runtime_shaders`, decision D1)**
**Plan:** [[M0 Plan - Scaffold and GPUI Window]] (step-by-step, with status per task)

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

- **Default GPUI build requires FULL Xcode** (not just Command Line Tools). GPUI's macOS renderer (`gpui_macos`)
  compiles **Metal shaders** at build time via `xcrun metal`, which ships only with Xcode. Without it
  the build fails: `unable to find utility "metal"`. Fix: install Xcode, then
  `sudo xcode-select -s /Applications/Xcode.app/Contents/Developer`, then rebuild.
- **Found 2026-10-12:** `gpui_platform`'s **`runtime_shaders`** feature compiles the shaders at app
  startup instead, so the build works without Xcode. It compiled and linked here; launch is not yet verified.
  `gpui_platform` also needs **`font-kit`** or no text renders. Both are in the [[M0 Plan - Scaffold and GPUI Window]].
- Note: at this rev the entry point is `gpui_platform::application()`, not `Application::new()`
  (see [[31 - API Fact Sheet (pinned rev)]]).
- **GPUI first build is heavy** (large dependency tree from the Zed repo) and pre-1.0. Resolved rev at
  first attempt: `1d2a4b3f` — pin it in `Cargo.toml` once the build is green.
- Needs stable Rust (have 1.97.1) + macOS/Linux.
- Verification is **hands-on-yours** (build & run, screenshot) — like v1's `vim`/banner checks.

## Related

- [[41 - Milestone 1 - One Terminal Pane]] · [[30 - Tech Stack]]
