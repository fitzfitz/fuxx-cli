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
[[41 - Milestone 1 - One Terminal Pane]]). Font: the bundled JetBrains Mono by default, set in Preferences or the
settings file (M6, M6b; below).

## Saved sessions: `serde` + `serde_json` (M7)

`serde = { version = "1.0.228", features = ["derive"] }` and `serde_json = "1.0.150"`, added 2026-10-27 for
`~/Library/Application Support/fuxx/sessions.json`. Both were already in `Cargo.lock` through GPUI, so nothing new
is downloaded (checked via Context7).

## Reader thread → UI channel: `futures`

`futures::channel::mpsc` (decision **D3**, 2026-10-26), declared as `futures = "0.3"` (locked at
`0.3.32`). It's what Zed uses and was already in `Cargo.lock` through GPUI. Exact APIs for all of
this: [[31 - API Fact Sheet (pinned rev)]]. Decisions: [[00 - Plan Tracker (Start Here)]].

## Notifications

fuxx's own banners from the app bundle through Apple's UserNotifications framework,
`objc2-user-notifications = "0.3.2"` (M11c): titled fuxx, a click shows the terminal that raised it. Outside the
app bundle, or when macOS says no, an `osascript` shell-out. Never `notify-rust` (v1 finding: it silently fails on
modern macOS — see memory `macos-notify-rust-broken`).

## macOS through objc2 (M10c onward)

`objc2 = "0.6.4"`, `objc2-app-kit`, `objc2-foundation`, `block2` (the versions GPUI already locks, default features
off, only the classes used) for the WebKit browser bar, sounds, sheets and menus; `objc2-web-kit = "0.3.2"` for the
agent browser (M10c), the in-pane browser (M11b) and the hidden view that renders Mermaid (M11ac).

## Smaller crates

- `memchr = "2.8"` — the byte scanners jump over plain text (M11n). Already in `Cargo.lock` via GPUI.
- `syntect = "5.3"` (pure-Rust `regex-fancy`, no Oniguruma) + `two-face = "0.5"` — syntax colours in file tabs (M11g).
- `similar = "3.2.1"` (`std` + `inline` only) — line-by-line comparison for live changes (M11af).

## Bundled assets

- **Lucide** outline icons (lucide-static 0.460.0, ISC) in `assets/icons/lucide/`, embedded and drawn with GPUI's
  `svg()`.
- The **Claude** and **Antigravity** logos in `assets/logos/` (sources in its `NOTICE.md`).
- **mermaid.js 12.2.0** (MIT) in `assets/vendor/mermaid/`, for offline diagrams in the Markdown Preview (M11ac).
- fuxx's **zsh integration** in `assets/shell-integration/zsh/` (OSC 133 marks, M11l).

## Agents

Agent tabs run the agents' own CLIs, nothing linked in: **Claude Code** and **Antigravity** (`agy`) in their
`--output-format stream-json` / `--input-format stream-json` modes (M11aa), each decoded by its provider file in
`src/providers/`. Recorded fixtures (`scripts/record-agent-fixture.py`) pin the formats in tests.

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
