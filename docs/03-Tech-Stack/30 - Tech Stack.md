---
tags: [moc, tech, dependencies]
---

# 30 - Tech Stack

Languages, frameworks, and crates. **Verify current versions/status before adding** (Context7 or
crates.io) — don't trust remembered version numbers.

## Language: Rust (edition 2024)

Kept from v1. Single native binary, strong correctness, cross-platform. The learning continues.

## App shell / UI: GPUI

Zed's GPU-accelerated Rust UI framework — window, layout, views, input, and our sidebar/panes/rings.
**Pre-1.0, no crates.io release**: consume as a **git dependency pinned to a Zed commit**:
`gpui = { git = "https://github.com/zed-industries/zed", rev = "<pinned>" }`. Expect breaking changes;
pin the rev. App entry: `Application::new().run(...)` → `open_window()` → root view.

## Terminal core: `alacritty_terminal`

The standalone grid + VT/escape parser powering Alacritty (also used by Zed, COSMIC term). Pure Rust,
no GUI deps. We feed it PTY bytes and read its grid to render.

## PTY: `portable-pty`

Spawns the shell/agent per session; bidirectional bytes. **Reused directly from v1.**

## Terminal rendering (we build, over the grid)

Glyph rendering in GPUI — likely `cosmic-text` (+ `glyphon`) for shaping/rasterization, or GPUI's own
text primitives. To be pinned down in [[41 - Milestone 1 - One Terminal Pane]].

## Notifications

`osascript` shell-out on macOS (v1 finding: `notify-rust` silently fails on modern macOS — see memory
`macos-notify-rust-broken`). Revisit for a proper app identity once bundled.

## Considered & not chosen

- **Swift + libghostty** (the cmux stack) — best rendering, but Swift + macOS-only. See
  [[70 - Terminal App Pivot — Vision and Stack]].
- **`gpui-ghostty`** — Ghostty rendering in GPUI, but needs the Zig toolchain + vendored build; we
  chose pure-Rust `alacritty_terminal` instead. Could revisit if our rendering proves too costly.
- **Fork Rio** (sugarloaf renderer) — viable alternative; not chosen.

## Related

- [[20 - Architecture Overview]] · [[40 - Roadmap]]
