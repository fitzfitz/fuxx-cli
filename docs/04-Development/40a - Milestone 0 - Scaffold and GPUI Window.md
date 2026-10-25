---
tags: [development, milestone]
status: done
---

# 40a - Milestone 0 - Scaffold and GPUI Window

**Status:: ✅ Done (2026-10-26). Builds without Xcode via `runtime_shaders` (decision D1); window confirmed on screen**
**Plan:** [[M0 Plan - Scaffold and GPUI Window]] (step-by-step, with status per task)

## Goal

Repurpose the crate and get a **blank GPUI window** to launch — prove the GPUI toolchain builds and
runs before adding the terminal core.

## Checklist

- [x] Decide the crate layout in this (repurposed) repo — keep v1 source or start the app crate fresh
      (v1 code is preserved in git history regardless)
- [x] Add GPUI as a **git dependency pinned to a Zed commit** (verify a known-good rev live)
- [x] Minimal app: `gpui_platform::application().run(...)` → `open_window()` → a root view showing a placeholder
- [x] `cargo run` opens a window (you confirm visually — GUI can't be verified by the AI)

## Success check

`cargo run` opens a GPUI window on screen with a placeholder view; closing it exits cleanly.

## Notes / risks

- **No full Xcode needed (resolved 2026-10-26).** GPUI's *default* macOS build compiles Metal shaders
  at build time with `xcrun metal`, which only ships with Xcode (`unable to find utility "metal"`).
  We enable `gpui_platform`'s **`runtime_shaders`** feature instead: shaders compile at app startup, so
  the Command Line Tools are enough. Built and launched here; window confirmed on screen (decision D1).
- `gpui_platform` also needs **`font-kit`** or no text renders. Both features are set in `Cargo.toml`.
- **Quitting:** macOS GPUI keeps apps alive with no windows, and GPUI ships no key bindings. M0 sets
  `QuitMode::LastWindowClosed` and binds **cmd-q** to a `Quit` action. Both confirmed on screen.
- Note: at this rev the entry point is `gpui_platform::application()`, not `Application::new()`
  (see [[31 - API Fact Sheet (pinned rev)]]).
- **GPUI first build is heavy** (large dependency tree from the Zed repo) and pre-1.0. Pinned to rev
  `1d2a4b3f` in `Cargo.toml`; `Cargo.lock` records `?rev=1d2a4b3f…`.
- Needs stable Rust (have 1.97.1) + macOS/Linux.
- Verification is **hands-on-yours** (build & run, screenshot) — like v1's `vim`/banner checks.

## Related

- [[41 - Milestone 1 - One Terminal Pane]] · [[30 - Tech Stack]]
