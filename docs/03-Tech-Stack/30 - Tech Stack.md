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
no GUI deps. We feed it PTY bytes and read its grid to render. **Version: `0.26.0`.**

## PTY: `portable-pty`

Spawns the shell/agent per session; bidirectional bytes. **Reused directly from v1.** **Version: `0.9.0`.**

## Terminal rendering (we build, over the grid)

**GPUI's own text system** (`shape_line` + `paint`), the same approach as Zed's terminal, with no
extra crate (decision **D2**, 2026-10-26; built and confirmed on screen in
[[41 - Milestone 1 - One Terminal Pane]]). Font: Menlo 14 px for now (config comes in M6).

## Saved sessions: `serde` + `serde_json` (M7)

`serde = { version = "1.0.228", features = ["derive"] }` and `serde_json = "1.0.150"`, added 2026-10-27 for
`~/Library/Application Support/fuxx/sessions.json`. Both were already in `Cargo.lock` through GPUI, so nothing new
is downloaded (checked via Context7).

## Reader thread → UI channel: `futures`

`futures::channel::mpsc` (decision **D3**, 2026-10-26), declared as `futures = "0.3"` (locked at
`0.3.32`). It's what Zed uses and was already in `Cargo.lock` through GPUI. Exact APIs for all of
this: [[31 - API Fact Sheet (pinned rev)]]. Decisions: [[00 - Plan Tracker (Start Here)]].

## Notifications

`osascript` shell-out on macOS (v1 finding: `notify-rust` silently fails on modern macOS — see memory
`macos-notify-rust-broken`). Revisit for a proper app identity once bundled.

## Settings, themes, fonts (M6)

- **fuxx's own settings file** `~/.config/fuxx/config` (Ghostty-style `key = value`, `src/settings.rs`); themes from
  `~/.config/fuxx/themes`, then six **built-in themes** in `assets/themes/` (from iTerm2-Color-Schemes, MIT; per-theme
  credits in `assets/themes/NOTICE.md`; TokyoNight is Apache-2.0 and ships its licence text).
- **Default font:** the bundled **JetBrains Mono 2.304** (Regular/Bold/Italic/Bold Italic) in `assets/fonts/`, SIL Open
  Font License 1.1 (licence text alongside). Registered with CoreText for the process at startup, like the icon font;
  an installed copy takes precedence. *(M6b, added 2026-10-27 on request.)*
- **Prompt icons:** the bundled **Symbols Nerd Font Mono** (Nerd Fonts 3.5.1) in `assets/fonts/`: MIT, plus icon
  sets under CC BY 4.0, Apache-2.0 and OFL (`NOTICE-NerdFonts-IconSets.md`). Registered with CoreText at startup
  (process scope) from a cached copy in `~/Library/Caches/fuxx/`. No new crates: two `extern "C"` calls.

## Considered & not chosen

- **Swift + libghostty** (the cmux stack) — best rendering, but Swift + macOS-only. See
  [[70 - Terminal App Pivot — Vision and Stack]].
- **`gpui-ghostty`** — Ghostty rendering in GPUI, but needs the Zig toolchain + vendored build; we
  chose pure-Rust `alacritty_terminal` instead. Could revisit if our rendering proves too costly.
- **Fork Rio** (sugarloaf renderer) — viable alternative; not chosen.

## Related

- [[20 - Architecture Overview]] · [[40 - Roadmap]]
