---
tags: [development, plan, milestone]
milestone: M1
status: done
updated: 2026-10-26
---

# M1 Plan — One Terminal Pane (make-or-break)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[41 - Milestone 1 - One Terminal Pane]] |
| **Status** | ✅ Done 2026-10-26 (commits `d4b972f`…`4a359c3` + docs on `m1-terminal-pane`) |
| **Tasks done** | 7 / 7 |
| **▶ Pick up here** | M1 complete → [[M2 Plan - Agent State Detection]] |
| **Decisions used** | ✅ **D2** = GPUI text system · ✅ **D3** = `futures` channel (see [[00 - Plan Tracker (Start Here)]]) |
| **Needs your eyes** | ✅ Task 6 confirmed by you on 2026-10-26 (all 12 checks) |

**Goal:** One real, usable terminal in the GPUI window. A login shell runs on a PTY, its output is
parsed by `alacritty_terminal`, the screen is drawn by a GPUI view, keystrokes go back to the
shell, and the window resizes the terminal.

**Architecture:** The pieces are split so that almost everything is unit-testable without a window:

```
 keystroke ──► keys.rs (pure) ──► bytes ──► TerminalCore.write ──► PTY ──► shell
                                                                           │ output bytes
 GPUI paint ◄── grid.rs spans/rects (pure) ◄── GridSnapshot ◄── Term ◄── reader thread (terminal.rs)
                     ▲ colors.rs (pure)                         │
                     └──────── TerminalView (GPUI) ◄── TermEvent channel (redraw, query replies, exit)
```

- `keys.rs`, `colors.rs`, `grid.rs`: pure functions, fully unit-tested.
- `pty.rs` + `terminal.rs`: the live terminal (no GPUI), tested against a real `/bin/sh` on a PTY.
- `terminal_view.rs`: the only GPUI-heavy file. It measures cells, paints the snapshot and handles keys.

**Tech Stack:** Rust 2024 · GPUI (pinned) · `alacritty_terminal 0.26.0` · `portable-pty 0.9.0` · `futures 0.3`.

**Spec:** [[41 - Milestone 1 - One Terminal Pane]] · [[20 - Architecture Overview]] · [[31 - API Fact Sheet (pinned rev)]]

## Global Constraints

- Everything from [[M0 Plan - Scaffold and GPUI Window]] (edition 2024, pinned GPUI rev, `font-kit`, zero warnings from our code, you verify GUI).
- New dependencies, exactly: `alacritty_terminal = "0.26.0"`, `portable-pty = "0.9.0"`, `futures = "0.3"`. Nothing else.
- Glyphs are drawn with **GPUI's text system** (D2). No `cosmic-text`/`glyphon`.
- The shell gets `TERM=xterm-256color` and `COLORTERM=truecolor`, always.
- Terminal query replies (`PtyWrite`, `ColorRequest`, `TextAreaSizeRequest`) are **always written back to the PTY**.
- The PTY writer is created **once** per session and kept. Dropping portable-pty's writer sends EOF (^D) to the child.
- `Term` is behind `alacritty_terminal::sync::FairMutex`. The event listener **never** locks it (it runs inside `advance`, while the lock is held).
- Out of scope (later milestones): scrollback UI, selection/copy/paste, mouse, URLs, fonts/config (M5–M6); multiple sessions (M3).
- **Dead-code warnings are expected during Tasks 1–4** (the modules aren't used yet). Task 5's build must be warning-free.

## Review Focus

Failure modes no unit test fully pins, most likely first:

1. **Programs that query the terminal hang** (zsh prompts, vim, Claude Code send DSR/DA). → Task 4 test `cursor_position_query_produces_a_pty_write_reply`; Task 5 writes replies back; Task 6 checks vim/claude start instantly.
2. **Claude Code's "synchronized updates" freeze the screen** if the end marker is late. → `TerminalCore::flush_stale_sync` (Task 4) on a 50 ms timer (Task 5); Task 6 runs `claude`.
3. **Tiny or zero-size window** → `resize` ignores 0×0 (Task 4 test `resize_changes_the_grid_and_ignores_zero`); `dims_for` clamps to ≥2 cols, ≥1 row (Task 5).
4. **Wide characters / emoji** misalign the grid → Task 3 test `wide_glyph_spacer_is_skipped`; Task 6 checks `echo 界🙂x`.
5. **Huge output bursts** (`seq 1 200000`) must stay responsive and end correctly → Task 6 check.

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `Cargo.toml` | Modify | +3 dependencies | 1 |
| `src/keys.rs` | Create | Keystroke → bytes (xterm conventions) | 1 |
| `src/colors.rs` | Create | Cell color → RGB (Alacritty palette, 256-color table) | 2 |
| `src/grid.rs` | Create | `GridSnapshot` (screen copy) + `bg_rects` / `text_spans` batching | 3 |
| `src/terminal.rs` | Create | `TermDims` (Task 3); `TerminalCore` + reader thread + `TermEvent` (Task 4) | 3, 4 |
| `src/pty.rs` | Create | Spawn login shell on a PTY; write; resize; kill on drop | 4 |
| `src/terminal_view.rs` | Create | GPUI view: measure, paint, keys, events, ticker | 5 |
| `src/main.rs` | Modify | Open the window with a `TerminalView` (replaces M0's placeholder) | 5 |

---

### Task 1: Dependencies + `keys.rs` (keystroke → bytes) — ✅

**Files:** Modify `Cargo.toml` · Create `src/keys.rs` · Modify `src/main.rs` (add `mod keys;`)

**Interfaces:**
- Produces: `pub fn key_to_bytes(key: &str, key_char: Option<&str>, ctrl: bool, alt: bool, shift: bool, app_cursor: bool) -> Option<Vec<u8>>`.
  `key`/`key_char` are GPUI's `Keystroke` fields. Callers skip cmd shortcuts and pass `alt` only for named keys.

> **Why this shape.** macOS gives GPUI `key: "c", key_char: None` for ctrl-c and
> `key: "a", key_char: Some("A")` for shift-a, so the terminal must compute control bytes itself.
> Arrows change form in "application cursor" mode (vim, less and Claude Code switch it on).
> Byte table: Zed's `crates/terminal/src/mappings/keys.rs`, with three of its bugs fixed (see [[31 - API Fact Sheet (pinned rev)]]).

- [x] **Step 1: Add the dependencies.** Append to `[dependencies]` in `Cargo.toml`:

```toml
# Terminal core: screen grid + VT/escape parser (the engine behind Alacritty and Zed's terminal).
alacritty_terminal = "0.26.0"
# Pseudo-terminal: spawns the shell and pipes bytes (reused from v1).
portable-pty = "0.9.0"
# Channel from the PTY reader thread to the UI (decision D3). Already in Cargo.lock via GPUI.
futures = "0.3"
```

- [x] **Step 2: Write the failing tests.** Create `src/keys.rs` containing only this test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn k(key: &str, key_char: Option<&str>) -> Option<Vec<u8>> {
        key_to_bytes(key, key_char, false, false, false, false)
    }

    #[test]
    fn printable_chars_pass_through() {
        assert_eq!(k("a", Some("a")), Some(b"a".to_vec()));
        assert_eq!(k("a", Some("A")), Some(b"A".to_vec())); // shift-a
        assert_eq!(k("!", Some("!")), Some(b"!".to_vec()));
        assert_eq!(k("s", Some("ß")), Some("ß".as_bytes().to_vec())); // option-s
    }

    #[test]
    fn enter_backspace_tab_escape() {
        assert_eq!(k("enter", Some("\n")), Some(b"\r".to_vec()));
        assert_eq!(k("backspace", None), Some(b"\x7f".to_vec()));
        assert_eq!(k("tab", Some("\t")), Some(b"\t".to_vec()));
        assert_eq!(k("escape", None), Some(b"\x1b".to_vec()));
        assert_eq!(key_to_bytes("tab", Some("\t"), false, false, true, false), Some(b"\x1b[Z".to_vec()));
    }

    #[test]
    fn arrows_respect_app_cursor_mode() {
        assert_eq!(k("up", None), Some(b"\x1b[A".to_vec()));
        assert_eq!(key_to_bytes("up", None, false, false, false, true), Some(b"\x1bOA".to_vec()));
        assert_eq!(key_to_bytes("left", None, false, false, false, true), Some(b"\x1bOD".to_vec()));
    }

    #[test]
    fn ctrl_letters_become_control_bytes() {
        assert_eq!(key_to_bytes("c", None, true, false, false, false), Some(vec![0x03]));
        assert_eq!(key_to_bytes("d", None, true, false, false, false), Some(vec![0x04]));
        assert_eq!(key_to_bytes("a", None, true, false, true, false), Some(vec![0x01])); // ctrl-shift-a
        assert_eq!(key_to_bytes("[", None, true, false, false, false), Some(vec![0x1b]));
    }

    #[test]
    fn modified_navigation_keys_use_csi_modifier_codes() {
        assert_eq!(key_to_bytes("up", None, true, false, false, false), Some(b"\x1b[1;5A".to_vec()));
        assert_eq!(key_to_bytes("left", None, false, true, false, false), Some(b"\x1b[1;3D".to_vec()));
        assert_eq!(key_to_bytes("f5", None, false, false, true, false), Some(b"\x1b[15;2~".to_vec()));
    }

    #[test]
    fn alt_prefixes_escape_for_named_keys() {
        assert_eq!(key_to_bytes("backspace", None, false, true, false, false), Some(b"\x1b\x7f".to_vec()));
        assert_eq!(key_to_bytes("space", Some(" "), false, true, false, false), Some(b"\x1b ".to_vec()));
    }

    #[test]
    fn unmapped_combinations_send_nothing() {
        assert_eq!(key_to_bytes("enter", Some("\n"), true, false, false, false), None);
        assert_eq!(k("unknown-key", None), None);
    }
}
```

…and register it at the top of `src/main.rs`:

```rust
mod keys;
```

- [x] **Step 3: Run them and watch them fail**

```bash
cargo test keys
```
Expected: compile error `cannot find function \`key_to_bytes\` in this scope` (red).

- [x] **Step 4: Implement.** Put this **above** the test module in `src/keys.rs`:

```rust
//! Translate a GPUI keystroke into the bytes a terminal program expects.
//!
//! Pure function, no GPUI types, so it is unit-testable. The caller skips cmd (platform)
//! shortcuts, which belong to the app, and passes `alt` only for named keys (arrows, enter, ...);
//! for character keys macOS already composed the Option character into `key_char`.
//! Byte table follows Zed's `crates/terminal/src/mappings/keys.rs` (xterm conventions).

pub fn key_to_bytes(
    key: &str,
    key_char: Option<&str>,
    ctrl: bool,
    alt: bool,
    shift: bool,
    app_cursor: bool,
) -> Option<Vec<u8>> {
    let none = !ctrl && !alt && !shift;
    let only_ctrl = ctrl && !alt && !shift;
    let only_alt = alt && !ctrl && !shift;
    let only_shift = shift && !ctrl && !alt;
    let s = |b: &[u8]| Some(b.to_vec());

    // 1. Keys with fixed meanings per exact modifier combination.
    match key {
        "tab" if none => return s(b"\t"),
        "tab" if only_shift => return s(b"\x1b[Z"),
        "escape" if none => return s(b"\x1b"),
        "enter" if none => return s(b"\r"),
        "enter" if only_shift => return s(b"\n"),
        "enter" if only_alt => return s(b"\x1b\r"),
        "backspace" if none || only_shift => return s(b"\x7f"),
        "backspace" if only_ctrl => return s(b"\x08"),
        "backspace" if only_alt => return s(b"\x1b\x7f"),
        "space" if only_ctrl => return s(b"\x00"),
        _ => {}
    }

    // 2. Unmodified navigation / function keys. Arrows + home/end switch to SS3 form in
    //    application-cursor mode (vim, less, Claude Code's TUI set it).
    if none {
        let ss3 = app_cursor;
        let seq: Option<&[u8]> = match key {
            "up" => Some(if ss3 { b"\x1bOA" } else { b"\x1b[A" }),
            "down" => Some(if ss3 { b"\x1bOB" } else { b"\x1b[B" }),
            "right" => Some(if ss3 { b"\x1bOC" } else { b"\x1b[C" }),
            "left" => Some(if ss3 { b"\x1bOD" } else { b"\x1b[D" }),
            "home" => Some(if ss3 { b"\x1bOH" } else { b"\x1b[H" }),
            "end" => Some(if ss3 { b"\x1bOF" } else { b"\x1b[F" }),
            "insert" => Some(b"\x1b[2~"),
            "delete" => Some(b"\x1b[3~"),
            "pageup" => Some(b"\x1b[5~"),
            "pagedown" => Some(b"\x1b[6~"),
            "f1" => Some(b"\x1bOP"),
            "f2" => Some(b"\x1bOQ"),
            "f3" => Some(b"\x1bOR"),
            "f4" => Some(b"\x1bOS"),
            "f5" => Some(b"\x1b[15~"),
            "f6" => Some(b"\x1b[17~"),
            "f7" => Some(b"\x1b[18~"),
            "f8" => Some(b"\x1b[19~"),
            "f9" => Some(b"\x1b[20~"),
            "f10" => Some(b"\x1b[21~"),
            "f11" => Some(b"\x1b[23~"),
            "f12" => Some(b"\x1b[24~"),
            _ => None,
        };
        if let Some(seq) = seq {
            return Some(seq.to_vec());
        }
    }

    // 3. Ctrl + single character -> C0 control byte (ctrl-c = 0x03). Case-insensitive because
    //    macOS reports ctrl-shift-a as key "a" + shift.
    if ctrl && !alt && key.chars().count() == 1 {
        let c = key.chars().next().unwrap();
        let b = match c {
            'a'..='z' => Some(c as u8 - b'a' + 1),
            'A'..='Z' => Some(c as u8 - b'A' + 1),
            '@' => Some(0x00),
            '[' => Some(0x1b),
            '\\' => Some(0x1c),
            ']' => Some(0x1d),
            '^' => Some(0x1e),
            '_' => Some(0x1f),
            '?' => Some(0x7f),
            _ => None,
        };
        if let Some(b) = b {
            return Some(vec![b]);
        }
    }

    // 4. Modified navigation / function keys: CSI with modifier code m = 1 + shift + 2*alt + 4*ctrl.
    if ctrl || alt || shift {
        let m = 1 + (shift as u8) + ((alt as u8) << 1) + ((ctrl as u8) << 2);
        let seq = match key {
            "up" => Some(format!("\x1b[1;{m}A")),
            "down" => Some(format!("\x1b[1;{m}B")),
            "right" => Some(format!("\x1b[1;{m}C")),
            "left" => Some(format!("\x1b[1;{m}D")),
            "home" => Some(format!("\x1b[1;{m}H")),
            "end" => Some(format!("\x1b[1;{m}F")),
            "f1" => Some(format!("\x1b[1;{m}P")),
            "f2" => Some(format!("\x1b[1;{m}Q")),
            "f3" => Some(format!("\x1b[1;{m}R")),
            "f4" => Some(format!("\x1b[1;{m}S")),
            "f5" => Some(format!("\x1b[15;{m}~")),
            "f6" => Some(format!("\x1b[17;{m}~")),
            "f7" => Some(format!("\x1b[18;{m}~")),
            "f8" => Some(format!("\x1b[19;{m}~")),
            "f9" => Some(format!("\x1b[20;{m}~")),
            "f10" => Some(format!("\x1b[21;{m}~")),
            "f11" => Some(format!("\x1b[23;{m}~")),
            "f12" => Some(format!("\x1b[24;{m}~")),
            "insert" => Some(format!("\x1b[2;{m}~")),
            "delete" => Some(format!("\x1b[3;{m}~")),
            "pageup" => Some(format!("\x1b[5;{m}~")),
            "pagedown" => Some(format!("\x1b[6;{m}~")),
            _ => None,
        };
        if let Some(seq) = seq {
            return Some(seq.into_bytes());
        }
    }

    // 5. Alt as Meta: ESC prefix (only reached for named keys, see module docs).
    if alt && !ctrl {
        let base = if key == "space" { " " } else { key };
        if base.len() == 1 && base.is_ascii() {
            let ch = if shift { base.to_ascii_uppercase() } else { base.to_string() };
            return Some(format!("\x1b{ch}").into_bytes());
        }
    }

    // 6. Plain text: whatever character macOS produced (handles shift, symbols, Option chars).
    if !ctrl {
        if let Some(t) = key_char {
            if !t.is_empty() {
                return Some(t.as_bytes().to_vec());
            }
        }
    }
    None
}
```

- [x] **Step 5: Run the tests**

```bash
cargo test keys
```
Expected: `test result: ok. 7 passed`. A `function key_to_bytes is never used` warning is expected until Task 5.

- [x] **Step 6: Commit** *(when you say so)*: `git add Cargo.toml Cargo.lock src/keys.rs src/main.rs && git commit -m "feat(m1): keystroke-to-bytes mapping with tests"`

### Task 2: `colors.rs` (cell color → RGB) — ✅

**Files:** Create `src/colors.rs` · Modify `src/main.rs` (add `mod colors;`)

**Interfaces:**
- Produces: `pub const FOREGROUND: Rgb`, `pub const BACKGROUND: Rgb`, `pub fn dim(Rgb) -> Rgb`,
  `pub fn indexed_rgb(u8) -> Rgb`, `pub fn default_for_index(usize) -> Rgb`,
  `pub fn resolve_index(usize, &Colors) -> Rgb`, `pub fn resolve(Color, &Colors) -> Rgb`
  (`Rgb`, `Color` from `alacritty_terminal::vte::ansi`; `Colors` from `alacritty_terminal::term::color`).

> **Why.** `Term::colors()` is all `None` unless a program overrode a color, so the palette is
> ours: Alacritty's defaults plus the standard xterm 256-color cube and gray ramp.

- [x] **Step 1: Write the failing tests.** Create `src/colors.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::NamedColor;

    #[test]
    fn palette_and_cube_and_gray_ramp() {
        assert_eq!(indexed_rgb(1), hex(0xac4242));
        assert_eq!(indexed_rgb(196), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(indexed_rgb(231), Rgb { r: 255, g: 255, b: 255 });
        assert_eq!(indexed_rgb(232), Rgb { r: 8, g: 8, b: 8 });
        assert_eq!(indexed_rgb(255), Rgb { r: 238, g: 238, b: 238 });
    }

    #[test]
    fn named_defaults() {
        let none = Colors::default();
        assert_eq!(resolve(Color::Named(NamedColor::Foreground), &none), FOREGROUND);
        assert_eq!(resolve(Color::Named(NamedColor::Background), &none), BACKGROUND);
        assert_eq!(resolve(Color::Named(NamedColor::Red), &none), hex(0xac4242));
    }

    #[test]
    fn spec_colors_pass_through_and_overrides_win() {
        let mut colors = Colors::default();
        let pink = Rgb { r: 1, g: 2, b: 3 };
        assert_eq!(resolve(Color::Spec(pink), &colors), pink);
        colors[NamedColor::Red] = Some(pink);
        assert_eq!(resolve(Color::Named(NamedColor::Red), &colors), pink);
    }
}
```

…and add `mod colors;` to `src/main.rs`.

- [x] **Step 2: Run:** `cargo test colors`. Expected: compile errors (`indexed_rgb`, `resolve`, `FOREGROUND` not found).

- [x] **Step 3: Implement.** Put this above the test module:

```rust
//! Resolve `alacritty_terminal` cell colors to concrete RGB.
//!
//! `Term::colors()` only holds colors a program overrode (OSC 4/10/11/12) and is otherwise all
//! `None`, so we supply the palette ourselves: Alacritty's defaults
//! (alacritty/src/config/color.rs). Pure, no GPUI types.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, Rgb};

const fn hex(v: u32) -> Rgb {
    Rgb { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8 }
}

pub const FOREGROUND: Rgb = hex(0xd8d8d8);
pub const BACKGROUND: Rgb = hex(0x181818);

/// 0-7 normal, 8-15 bright.
const ANSI_16: [Rgb; 16] = [
    hex(0x181818), hex(0xac4242), hex(0x90a959), hex(0xf4bf75),
    hex(0x6a9fb5), hex(0xaa759f), hex(0x75b5aa), hex(0xd8d8d8),
    hex(0x6b6b6b), hex(0xc55555), hex(0xaac474), hex(0xfeca88),
    hex(0x82b8c8), hex(0xc28cb8), hex(0x93d3c3), hex(0xf8f8f8),
];

/// Alacritty dims colors to 66% brightness.
const DIM_FACTOR: f32 = 0.66;

pub fn dim(c: Rgb) -> Rgb {
    let f = |v: u8| (v as f32 * DIM_FACTOR) as u8;
    Rgb { r: f(c.r), g: f(c.g), b: f(c.b) }
}

/// The xterm 256-color table: 16 palette colors, a 6x6x6 cube, then a 24-step gray ramp.
pub fn indexed_rgb(i: u8) -> Rgb {
    match i {
        0..=15 => ANSI_16[i as usize],
        16..=231 => {
            let i = i - 16;
            let (r, g, b) = (i / 36, (i % 36) / 6, i % 6);
            let step = |v: u8| if v == 0 { 0 } else { v * 40 + 55 };
            Rgb { r: step(r), g: step(g), b: step(b) }
        }
        232..=255 => {
            let v = (i - 232) * 10 + 8;
            Rgb { r: v, g: v, b: v }
        }
    }
}

/// Default for any index in alacritty's 269-slot color table
/// (0-255 indexed, 256 fg, 257 bg, 258 cursor, 259-266 dim 0-7, 267 bright fg, 268 dim fg).
pub fn default_for_index(idx: usize) -> Rgb {
    match idx {
        0..=255 => indexed_rgb(idx as u8),
        256 | 258 | 267 => FOREGROUND,
        257 => BACKGROUND,
        259..=266 => dim(ANSI_16[idx - 259]),
        268 => dim(FOREGROUND),
        _ => FOREGROUND,
    }
}

/// A program's override wins; otherwise our palette.
pub fn resolve_index(idx: usize, overrides: &Colors) -> Rgb {
    overrides[idx].unwrap_or_else(|| default_for_index(idx))
}

pub fn resolve(color: Color, overrides: &Colors) -> Rgb {
    match color {
        Color::Spec(rgb) => rgb,
        Color::Indexed(i) => resolve_index(i as usize, overrides),
        Color::Named(n) => resolve_index(n as usize, overrides),
    }
}
```

- [x] **Step 4: Run:** `cargo test colors`. Expected: `test result: ok. 3 passed`.

- [x] **Step 5: Commit** *(when you say so)*: `git add src/colors.rs src/main.rs && git commit -m "feat(m1): terminal color palette with tests"`

### Task 3: `grid.rs` (screen snapshot + paint batching) — ✅

**Files:** Create `src/terminal.rs` (only `TermDims` for now) · Create `src/grid.rs` · Modify `src/main.rs` (add `mod terminal; mod grid;`)

**Interfaces:**
- Consumes: `colors::{FOREGROUND, BACKGROUND, resolve, dim}` (Task 2).
- Produces:
  - `terminal::TermDims { pub cols: usize, pub rows: usize }` (implements alacritty's `Dimensions`; `Copy + Eq`).
  - `grid::CellView { ch, fg, bg, bold, italic, underline, spacer }` + `CellView::BLANK`.
  - `grid::CursorView { row, col, shape: CursorShape }`.
  - `grid::GridSnapshot { cols, rows, lines: Vec<Vec<CellView>>, cursor: Option<CursorView> }`.
  - `pub fn snapshot<L: EventListener>(term: &Term<L>) -> GridSnapshot`.
  - `grid::BgRect { col, cells, color }` + `pub fn bg_rects(&[CellView], default_bg: Rgb) -> Vec<BgRect>`.
  - `grid::TextSpan { col, cells, text, fg, bold, italic, underline }` + `pub fn text_spans(&[CellView]) -> Vec<TextSpan>`.

> **Why a snapshot.** The `Term` lives behind a mutex the reader thread also needs. We copy the
> visible screen out quickly, release the lock, and paint from the copy. Batching same-style cells
> into spans means one GPUI `shape_line` per span instead of one per cell (Zed does the same).

- [x] **Step 1: Create `src/terminal.rs` with just the size type** (Task 4 fills in the rest):

```rust
//! One live terminal (filled in by Task 4). For now: the size type.

use alacritty_terminal::grid::Dimensions;

/// Terminal size in cells. Implements alacritty's `Dimensions` so `Term` can use it directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TermDims {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for TermDims {
    fn total_lines(&self) -> usize {
        self.rows // Term::new/resize only read screen_lines + columns
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}
```

- [x] **Step 2: Write the failing tests.** Create `src/grid.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TermDims;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    fn term_with(bytes: &[u8]) -> Term<VoidListener> {
        let mut term = Term::new(Config::default(), &TermDims { cols: 10, rows: 3 }, VoidListener);
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        processor.advance(&mut term, bytes);
        term
    }

    fn cell(ch: char, fg: Rgb, bg: Rgb) -> CellView {
        CellView { ch, fg, bg, ..CellView::BLANK }
    }

    const RED: Rgb = Rgb { r: 0xac, g: 0x42, b: 0x42 };

    #[test]
    fn snapshot_copies_text_colors_and_cursor() {
        let snap = snapshot(&term_with(b"hi\x1b[31mR"));
        assert_eq!((snap.cols, snap.rows), (10, 3));
        assert_eq!(snap.lines[0][0].ch, 'h');
        assert_eq!(snap.lines[0][1].ch, 'i');
        assert_eq!(snap.lines[0][2].ch, 'R');
        assert_eq!(snap.lines[0][2].fg, RED);
        assert_eq!(snap.lines[0][0].fg, colors::FOREGROUND);
        assert_eq!(snap.cursor.map(|c| (c.row, c.col)), Some((0, 3)));
    }

    #[test]
    fn snapshot_applies_inverse_and_hidden_cursor() {
        let snap = snapshot(&term_with(b"\x1b[7mX\x1b[?25l"));
        assert_eq!(snap.lines[0][0].fg, colors::BACKGROUND);
        assert_eq!(snap.lines[0][0].bg, colors::FOREGROUND);
        assert_eq!(snap.cursor, None);
    }

    #[test]
    fn bg_rects_merge_adjacent_same_color() {
        let d = colors::BACKGROUND;
        let line = [cell('a', RED, RED), cell('b', RED, RED), cell('c', RED, d), cell('d', RED, RED)];
        assert_eq!(
            bg_rects(&line, d),
            vec![BgRect { col: 0, cells: 2, color: RED }, BgRect { col: 3, cells: 1, color: RED }]
        );
    }

    #[test]
    fn text_spans_split_on_style_and_skip_leading_blanks() {
        let d = colors::BACKGROUND;
        let f = colors::FOREGROUND;
        let line = [cell(' ', f, d), cell('l', f, d), cell(' ', f, d), cell('s', f, d), cell(' ', f, d), cell('x', RED, d)];
        let spans = text_spans(&line);
        assert_eq!(spans.len(), 2);
        // inner space kept, trailing space trimmed
        assert_eq!((spans[0].col, spans[0].text.as_str(), spans[0].cells), (1, "l s", 3));
        assert_eq!((spans[1].col, spans[1].text.as_str(), spans[1].fg), (5, "x", RED));
    }

    #[test]
    fn blank_line_has_no_spans_or_rects() {
        let line = vec![CellView::BLANK; 10];
        assert!(text_spans(&line).is_empty());
        assert!(bg_rects(&line, colors::BACKGROUND).is_empty());
    }

    #[test]
    fn wide_glyph_spacer_is_skipped() {
        let snap = snapshot(&term_with("界a".as_bytes()));
        assert_eq!(snap.lines[0][0].ch, '界');
        assert!(snap.lines[0][1].spacer);
        let spans = text_spans(&snap.lines[0]);
        assert_eq!(spans[0].text, "界");
        assert_eq!((spans[1].col, spans[1].text.as_str()), (2, "a"));
    }
}
```

…and add `mod terminal;` and `mod grid;` to `src/main.rs`.

- [x] **Step 3: Run:** `cargo test grid`. Expected: compile errors (`snapshot`, `CellView`, `bg_rects`… not found).

- [x] **Step 4: Implement.** Put this above the test module in `src/grid.rs`:

```rust
//! A render-ready copy of the terminal screen, plus the batching that turns a line of cells
//! into a few background rectangles and text runs.
//!
//! Everything here is plain data (no GPUI), so the tricky parts of rendering are unit-tested.
//! The GPUI view only paints what these functions return.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};

use crate::colors;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellView {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Right half of a wide (CJK/emoji) glyph: draws nothing itself.
    pub spacer: bool,
}

impl CellView {
    pub const BLANK: CellView = CellView {
        ch: ' ',
        fg: colors::FOREGROUND,
        bg: colors::BACKGROUND,
        bold: false,
        italic: false,
        underline: false,
        spacer: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorView {
    pub row: usize,
    pub col: usize,
    pub shape: CursorShape,
}

#[derive(Clone, Debug)]
pub struct GridSnapshot {
    pub cols: usize,
    pub rows: usize,
    /// `lines[row][col]`, viewport coordinates (row 0 = top of the visible screen).
    pub lines: Vec<Vec<CellView>>,
    /// `None` when the program hid the cursor.
    pub cursor: Option<CursorView>,
}

/// Copy the visible screen out of the `Term` (call while holding its lock, then release it).
pub fn snapshot<L: EventListener>(term: &Term<L>) -> GridSnapshot {
    let rows = term.screen_lines();
    let cols = term.columns();
    let content = term.renderable_content();
    let overrides = content.colors;
    // Grid lines are negative when scrolled back; viewport row = line + display_offset.
    let offset = content.display_offset as i32;
    let mut lines = vec![vec![CellView::BLANK; cols]; rows];

    for indexed in content.display_iter {
        let row = indexed.point.line.0 + offset;
        let col = indexed.point.column.0;
        if row < 0 || row as usize >= rows || col >= cols {
            continue;
        }
        let cell = indexed.cell;
        let flags = cell.flags;
        let mut fg = colors::resolve(cell.fg, overrides);
        let mut bg = colors::resolve(cell.bg, overrides);
        if flags.contains(Flags::DIM) {
            fg = colors::dim(fg);
        }
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        lines[row as usize][col] = CellView {
            ch: if flags.contains(Flags::HIDDEN) { ' ' } else { cell.c },
            fg,
            bg,
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            spacer: flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
        };
    }

    let c = content.cursor;
    let cursor_row = c.point.line.0 + offset;
    let cursor = (c.shape != CursorShape::Hidden && cursor_row >= 0 && (cursor_row as usize) < rows)
        .then(|| CursorView { row: cursor_row as usize, col: c.point.column.0, shape: c.shape });

    GridSnapshot { cols, rows, lines, cursor }
}

/// A horizontal span of cells sharing a non-default background color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BgRect {
    pub col: usize,
    pub cells: usize,
    pub color: Rgb,
}

/// A horizontal span of cells drawn with one text style (shaped as one GPUI line).
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub col: usize,
    pub cells: usize,
    pub text: String,
    pub fg: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Merge adjacent same-colored backgrounds; the default background is painted once for the
/// whole view, so it's skipped here.
pub fn bg_rects(line: &[CellView], default_bg: Rgb) -> Vec<BgRect> {
    let mut out: Vec<BgRect> = Vec::new();
    for (col, cell) in line.iter().enumerate() {
        if cell.bg == default_bg {
            continue;
        }
        match out.last_mut() {
            Some(r) if r.color == cell.bg && r.col + r.cells == col => r.cells += 1,
            _ => out.push(BgRect { col, cells: 1, color: cell.bg }),
        }
    }
    out
}

/// Group contiguous same-style cells into spans. Spaces extend a span but never start one,
/// so blank areas cost nothing; wide-glyph spacers are skipped (and so end the span).
pub fn text_spans(line: &[CellView]) -> Vec<TextSpan> {
    let mut out: Vec<TextSpan> = Vec::new();
    for (col, cell) in line.iter().enumerate() {
        if cell.spacer {
            continue;
        }
        let continues = |s: &TextSpan| {
            s.col + s.cells == col
                && s.fg == cell.fg
                && s.bold == cell.bold
                && s.italic == cell.italic
                && s.underline == cell.underline
        };
        match out.last_mut() {
            Some(s) if continues(s) => {
                s.text.push(cell.ch);
                s.cells += 1;
            }
            _ if cell.ch == ' ' && !cell.underline => {}
            _ => out.push(TextSpan {
                col,
                cells: 1,
                text: cell.ch.to_string(),
                fg: cell.fg,
                bold: cell.bold,
                italic: cell.italic,
                underline: cell.underline,
            }),
        }
    }
    // Trailing spaces draw nothing (unless underlined): drop them.
    for s in &mut out {
        if !s.underline {
            let trimmed = s.text.trim_end_matches(' ').len();
            s.cells -= s.text.len() - trimmed;
            s.text.truncate(trimmed);
        }
    }
    out
}
```

- [x] **Step 5: Run:** `cargo test grid`. Expected: `test result: ok. 6 passed`.

- [x] **Step 6: Commit** *(when you say so)*: `git add src/terminal.rs src/grid.rs src/main.rs && git commit -m "feat(m1): grid snapshot and paint batching with tests"`

### Task 4: `pty.rs` + `TerminalCore` (the live terminal) — ✅

**Files:** Create `src/pty.rs` · Replace `src/terminal.rs` (full version) · Modify `src/main.rs` (add `mod pty;`)

**Interfaces:**
- Consumes: `TermDims` (Task 3), `grid::{snapshot, GridSnapshot}` (Task 3), `colors::resolve_index` (Task 2).
- Produces:
  - `pty::shell_command(cwd: Option<&Path>) -> CommandBuilder`: login shell in `cwd` or `$HOME`, with `TERM`/`COLORTERM` set.
  - `pty::Pty::{spawn(cmd, TermDims) -> Result<(Pty, Box<dyn Read + Send>), String>, write(&mut self, &[u8]), resize(&self, TermDims)}`. Kills the child on drop.
  - `terminal::TermEvent { Term(alacritty_terminal::event::Event), Output, Exited }`.
  - `terminal::TerminalCore` (`Clone`, all `Arc`s):
    `spawn(CommandBuilder, TermDims, UnboundedSender<TermEvent>) -> Result<Self, String>`,
    `write(&self, &[u8])`, `resize(&self, TermDims)` (no-op if unchanged or zero), `dims() -> TermDims`,
    `mode() -> TermMode`, `snapshot() -> GridSnapshot`, `color(usize) -> Rgb`, `flush_stale_sync() -> bool`.

> **How the threads fit.** A dedicated `fuxx-pty-reader` thread blocks on `read`, feeds bytes to
> the parser (`Processor::advance`) under the `Term` lock, releases the lock, then sends
> `TermEvent::Output` so the UI redraws. At EOF it sends `Exited`. The parser lives *next to* the
> `Term` under the same lock, so a timer can flush a stuck synchronized update (`flush_stale_sync`).
> macOS returns `EIO` when the child exits, and portable-pty already turns that into `Ok(0)` (EOF).

- [x] **Step 1: Create `src/pty.rs`** (exercised by Task 4's tests through `TerminalCore`):

```rust
//! A child process on a pseudo-terminal (portable-pty, as in v1).

use std::io::{Read, Write};
use std::path::Path;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::terminal::TermDims;

/// The user's login shell (`$SHELL`, run as `-zsh` etc.), started in `cwd` or `$HOME`.
pub fn shell_command(cwd: Option<&Path>) -> CommandBuilder {
    let mut cmd = CommandBuilder::new_default_prog();
    match cwd {
        Some(dir) => cmd.cwd(dir),
        None => {
            if let Some(home) = std::env::var_os("HOME") {
                cmd.cwd(home);
            }
        }
    }
    // Always set these: an app launched from Finder has no TERM, one launched from another
    // terminal inherits *that* terminal's TERM. `alacritty` terminfo isn't on stock macOS.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd
}

fn pty_size(dims: TermDims) -> PtySize {
    PtySize { rows: dims.rows as u16, cols: dims.cols as u16, pixel_width: 0, pixel_height: 0 }
}

pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    /// Kept for the whole session: dropping portable-pty's writer sends EOF (^D) to the child.
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

impl Pty {
    /// Spawn `cmd`; returns the handle plus the output reader (for a dedicated thread).
    pub fn spawn(cmd: CommandBuilder, dims: TermDims) -> Result<(Pty, Box<dyn Read + Send>), String> {
        let pair = native_pty_system().openpty(pty_size(dims)).map_err(|e| e.to_string())?;
        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        // Drop our copy of the slave: the reader only reaches EOF once every slave handle closes.
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        Ok((Pty { master: pair.master, writer, child }, reader))
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writer.write_all(bytes).map_err(|e| e.to_string())?;
        self.writer.flush().map_err(|e| e.to_string())
    }

    /// Tell the child its new size (it receives SIGWINCH).
    pub fn resize(&self, dims: TermDims) -> Result<(), String> {
        self.master.resize(pty_size(dims)).map_err(|e| e.to_string())
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Closing a session must not leave its shell/agent running.
        let _ = self.child.kill();
    }
}
```

- [x] **Step 2: Write the failing tests.** Append this test module to the bottom of `src/terminal.rs`, and add `mod pty;` to `src/main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};
    use std::time::Duration;

    /// Poll the channel until `pred` matches or 5 s pass.
    fn wait_for(rx: &mut UnboundedReceiver<TermEvent>, mut pred: impl FnMut(&TermEvent) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(ev) if pred(&ev) => return true,
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        false
    }

    fn sh(script: &str) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", script]);
        cmd
    }

    fn screen_text(core: &TerminalCore) -> String {
        core.snapshot()
            .lines
            .iter()
            .map(|l| l.iter().map(|c| c.ch).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn child_output_reaches_the_screen_and_exit_is_reported() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(sh("printf 'hello fuxx'"), TermDims { cols: 40, rows: 5 }, tx).unwrap();
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Exited)));
        assert!(screen_text(&core).contains("hello fuxx"));
    }

    #[test]
    fn cursor_position_query_produces_a_pty_write_reply() {
        let (tx, mut rx) = unbounded();
        let _core = TerminalCore::spawn(sh("printf '\\033[6n'; sleep 1"), TermDims { cols: 40, rows: 5 }, tx).unwrap();
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Term(Event::PtyWrite(s)) if s == "\x1b[1;1R")));
    }

    #[test]
    fn written_bytes_reach_the_child() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(sh("read line; printf \"got:$line\""), TermDims { cols: 40, rows: 5 }, tx).unwrap();
        core.write(b"ping\r");
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Exited)));
        assert!(screen_text(&core).contains("got:ping"));
    }

    #[test]
    fn resize_changes_the_grid_and_ignores_zero() {
        let (tx, _rx) = unbounded();
        let core = TerminalCore::spawn(sh("sleep 1"), TermDims { cols: 40, rows: 5 }, tx).unwrap();
        core.resize(TermDims { cols: 0, rows: 0 });
        assert_eq!(core.dims(), TermDims { cols: 40, rows: 5 });
        core.resize(TermDims { cols: 60, rows: 10 });
        let snap = core.snapshot();
        assert_eq!((snap.cols, snap.rows), (60, 10));
    }

    #[test]
    fn app_cursor_mode_is_visible_to_the_ui() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(sh("printf '\\033[?1h'; sleep 1"), TermDims { cols: 40, rows: 5 }, tx).unwrap();
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Output)));
        assert!(core.mode().contains(TermMode::APP_CURSOR));
    }
}
```

- [x] **Step 3: Run:** `cargo test terminal`. Expected: compile errors (`TerminalCore`, `TermEvent` not found).

- [x] **Step 4: Implement.** Replace everything **above** the test module in `src/terminal.rs` with:

```rust
//! One live terminal: a PTY child, an `alacritty_terminal::Term` holding its screen, and a
//! reader thread feeding the child's output into the Term. No GPUI here: the view talks to it
//! through `TerminalCore` (cheap to clone, every field is an `Arc`) and an event channel.

use std::io::{ErrorKind, Read};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Processor, Rgb, StdSyncHandler};
use futures::channel::mpsc::UnboundedSender;
use portable_pty::CommandBuilder;

use crate::colors;
use crate::grid::{self, GridSnapshot};
use crate::pty::Pty;

/// Terminal size in cells. Implements alacritty's `Dimensions` so `Term` can use it directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TermDims {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for TermDims {
    fn total_lines(&self) -> usize {
        self.rows // Term::new/resize only read screen_lines + columns
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Everything the UI needs to hear about, in order.
#[derive(Debug)]
pub enum TermEvent {
    /// Something the `Term` raised while parsing (query replies, title, bell, ...).
    Term(Event),
    /// New output was parsed into the screen: redraw.
    Output,
    /// The child closed the PTY (shell exited).
    Exited,
}

/// Receives `Term` events. It runs *inside* `Processor::advance` while the Term lock is held,
/// so it must never lock the Term: it only forwards into the channel.
#[derive(Clone)]
pub struct Listener(UnboundedSender<TermEvent>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.0.unbounded_send(TermEvent::Term(event));
    }
}

/// The parser lives next to the Term under one lock so a stuck synchronized update can be
/// flushed from outside the reader thread (see `flush_stale_sync`).
struct Shared {
    term: Term<Listener>,
    processor: Processor<StdSyncHandler>,
}

#[derive(Clone)]
pub struct TerminalCore {
    shared: Arc<FairMutex<Shared>>,
    pty: Arc<Mutex<Pty>>,
    dims: Arc<Mutex<TermDims>>,
}

impl TerminalCore {
    pub fn spawn(
        cmd: CommandBuilder,
        dims: TermDims,
        events: UnboundedSender<TermEvent>,
    ) -> Result<Self, String> {
        let (pty, mut reader) = Pty::spawn(cmd, dims)?;
        let term = Term::new(Config::default(), &dims, Listener(events.clone()));
        let shared = Arc::new(FairMutex::new(Shared { term, processor: Processor::new() }));

        let reader_shared = shared.clone();
        std::thread::Builder::new()
            .name("fuxx-pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        // EOF. portable-pty already maps macOS's EIO-at-exit to Ok(0).
                        Ok(0) => break,
                        Ok(n) => {
                            {
                                let mut guard = reader_shared.lock();
                                let Shared { term, processor } = &mut *guard;
                                processor.advance(term, &buf[..n]);
                            } // lock released before waking the UI
                            let _ = events.unbounded_send(TermEvent::Output);
                        }
                        Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                let _ = events.unbounded_send(TermEvent::Exited);
            })
            .map_err(|e| e.to_string())?;

        Ok(Self { shared, pty: Arc::new(Mutex::new(pty)), dims: Arc::new(Mutex::new(dims)) })
    }

    /// Send bytes to the child (keystrokes, query replies).
    pub fn write(&self, bytes: &[u8]) {
        if let Ok(mut pty) = self.pty.lock() {
            let _ = pty.write(bytes);
        }
    }

    /// Resize screen + PTY. No-op when unchanged, so it's cheap to call every frame.
    pub fn resize(&self, dims: TermDims) {
        if dims.cols == 0 || dims.rows == 0 {
            return;
        }
        let mut current = self.dims.lock().unwrap();
        if *current == dims {
            return;
        }
        *current = dims;
        self.shared.lock().term.resize(dims);
        if let Ok(pty) = self.pty.lock() {
            let _ = pty.resize(dims);
        }
    }

    pub fn dims(&self) -> TermDims {
        *self.dims.lock().unwrap()
    }

    pub fn mode(&self) -> TermMode {
        *self.shared.lock().term.mode()
    }

    pub fn snapshot(&self) -> GridSnapshot {
        grid::snapshot(&self.shared.lock().term)
    }

    /// Color for an OSC 4/10/11/12 query reply.
    pub fn color(&self, index: usize) -> Rgb {
        colors::resolve_index(index, self.shared.lock().term.colors())
    }

    /// Synchronized updates (DEC mode 2026, used by Claude Code and most TUIs) buffer output
    /// until an end marker. If the marker never comes, the parser holds the frame until its
    /// deadline (~150 ms) *and* the next byte arrives. Called on a timer to flush it on time.
    /// Returns true if something was flushed (redraw needed).
    pub fn flush_stale_sync(&self) -> bool {
        let mut guard = self.shared.lock();
        let Shared { term, processor } = &mut *guard;
        match processor.sync_timeout().sync_timeout() {
            Some(deadline) if Instant::now() >= deadline => {
                processor.stop_sync(term);
                true
            }
            _ => false,
        }
    }
}
```

- [x] **Step 5: Run:** `cargo test terminal`. Expected: `test result: ok. 5 passed`. These spawn real `/bin/sh` processes on PTYs, so each takes up to ~1 s.

- [x] **Step 6: Run everything:** `cargo test`. Expected: `test result: ok. 21 passed`.

- [x] **Step 7: Commit** *(when you say so)*: `git add src/pty.rs src/terminal.rs src/main.rs && git commit -m "feat(m1): live terminal core (PTY + alacritty Term + reader thread) with tests"`

### Task 5: `TerminalView` + wire it into the window — ✅

**Files:** Create `src/terminal_view.rs` · Replace `src/main.rs`

**Interfaces:**
- Consumes: everything above.
- Produces: `terminal_view::TerminalView` with `pub fn spawn(window: &mut Window, cx: &mut App) -> Result<Entity<TerminalView>, String>` (starts a login shell, returns a focused view). M2–M4 build on it.

> **GPUI notes.** `canvas(prepaint, paint)` hands both closures the element's pixel `bounds`
> every frame. Prepaint measures the font, computes cols×rows and calls `core.resize` (a no-op
> when unchanged). Paint takes a snapshot and draws backgrounds, then text spans, then the cursor.
> The closures capture *clones* of `TerminalCore` (cheap `Arc`s), never the view itself, so
> there's no re-entrant borrow. `cx.spawn(async move |this, cx| …)` runs on the UI thread. One task
> drains `TermEvent`s and another ticks every 50 ms. Both are stored in fields because **dropping
> a GPUI `Task` cancels it**. Keys arrive in `on_key_down` only if no key binding matched (cmd-q is bound).

- [x] **Step 1: Create `src/terminal_view.rs`:**

```rust
//! The GPUI view for one terminal: paints a `GridSnapshot` and turns keystrokes into PTY bytes.

use std::time::Duration;

use alacritty_terminal::event::{Event, WindowSize};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Bounds, Context, Entity, FocusHandle, Font, FontStyle, FontWeight, Hsla, KeyDownEvent, Pixels,
    Rgba, SharedString, Task, TextAlign, TextRun, UnderlineStyle, Window, canvas, div, fill, font,
    point, prelude::*, px, size,
};

use crate::colors;
use crate::grid::{self, GridSnapshot};
use crate::keys;
use crate::pty;
use crate::terminal::{TermDims, TermEvent, TerminalCore};

const FONT_FAMILY: &str = "Menlo";
const FONT_SIZE: f32 = 14.0;
/// GPUI's default line height (1.618 x size) is too airy for a terminal.
const LINE_HEIGHT_FACTOR: f32 = 1.3;
/// How often to run housekeeping (flush stuck synchronized updates).
const TICK: Duration = Duration::from_millis(50);

pub struct TerminalView {
    core: TerminalCore,
    focus_handle: FocusHandle,
    title: Option<SharedString>,
    /// Dropping a GPUI `Task` cancels it, so these live as long as the view.
    _events: Task<()>,
    _ticker: Task<()>,
}

impl TerminalView {
    /// Start a login shell and wrap it in a new view entity (focused).
    pub fn spawn(window: &mut Window, cx: &mut App) -> Result<Entity<Self>, String> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        // 80x24 until the first paint measures the real size and resizes.
        let core = TerminalCore::spawn(pty::shell_command(None), TermDims { cols: 80, rows: 24 }, tx)?;
        Ok(cx.new(|cx| Self::new(core, rx, window, cx)))
    }

    fn new(
        core: TerminalCore,
        mut rx: UnboundedReceiver<TermEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Drain terminal events on the UI thread, in order.
        let events = cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this.update(cx, |this, cx| this.on_term_event(event, cx)).is_err() {
                    break; // view was dropped
                }
            }
        });

        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |this, cx| this.on_tick(cx)).is_err() {
                    break;
                }
            }
        });

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self { core, focus_handle, title: None, _events: events, _ticker: ticker }
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        if self.core.flush_stale_sync() {
            cx.notify();
        }
    }

    fn on_term_event(&mut self, event: TermEvent, cx: &mut Context<Self>) {
        match event {
            TermEvent::Output | TermEvent::Term(Event::Wakeup) => cx.notify(),
            // Replies to terminal queries (cursor position, device attributes, ...) MUST go back
            // to the child, or programs that ask (shells, vim, Claude Code) hang waiting.
            TermEvent::Term(Event::PtyWrite(text)) => self.core.write(text.as_bytes()),
            TermEvent::Term(Event::ColorRequest(index, format)) => {
                let reply = format(self.core.color(index));
                self.core.write(reply.as_bytes());
            }
            TermEvent::Term(Event::TextAreaSizeRequest(format)) => {
                let dims = self.core.dims();
                // Pixel cell size isn't tracked; cell counts are what programs actually use.
                let reply = format(WindowSize {
                    num_lines: dims.rows as u16,
                    num_cols: dims.cols as u16,
                    cell_width: 0,
                    cell_height: 0,
                });
                self.core.write(reply.as_bytes());
            }
            TermEvent::Term(Event::Title(title)) => {
                self.title = Some(title.into());
                cx.notify();
            }
            TermEvent::Term(Event::ResetTitle) => {
                self.title = None;
                cx.notify();
            }
            // One window, one shell (M1): when the shell exits, quit like a closed terminal.
            TermEvent::Exited => cx.quit(),
            TermEvent::Term(_) => {}
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
        if m.platform {
            return; // cmd-shortcuts belong to the app (cmd-q, later cmd-t ...)
        }
        // Option on a *character* key already composed a character into key_char (option-s = ß);
        // only named keys (arrows, backspace, enter, ...) get Alt-as-Meta treatment.
        let named_key = keystroke.key.chars().count() > 1;
        let app_cursor = self.core.mode().contains(TermMode::APP_CURSOR);
        if let Some(bytes) = keys::key_to_bytes(
            &keystroke.key,
            keystroke.key_char.as_deref(),
            m.control,
            m.alt && named_key,
            m.shift,
            app_cursor,
        ) {
            self.core.write(&bytes);
            cx.stop_propagation();
        }
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.title.clone().unwrap_or_else(|| "fuxx".into());
        window.set_window_title(&title);

        let focused = self.focus_handle.is_focused(window);
        let resize_core = self.core.clone();
        let paint_core = self.core.clone();

        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("Terminal")
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                canvas(
                    // prepaint: measure cells, fit the terminal to the space we were given.
                    move |bounds, window, _cx| {
                        let metrics = CellMetrics::measure(window);
                        resize_core.resize(metrics.dims_for(bounds));
                        metrics
                    },
                    // paint: draw the current screen.
                    move |bounds, metrics, window, cx| {
                        paint_grid(paint_core.snapshot(), &metrics, bounds, focused, window, cx);
                    },
                )
                .size_full(),
            )
    }
}

/// Monospace cell geometry for the terminal font.
struct CellMetrics {
    font: Font,
    font_size: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
}

impl CellMetrics {
    fn measure(window: &Window) -> Self {
        let font = font(FONT_FAMILY);
        let font_size = px(FONT_SIZE);
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        let cell_width = text_system.em_advance(font_id, font_size).unwrap_or(px(FONT_SIZE * 0.6));
        let line_height = (font_size * LINE_HEIGHT_FACTOR).round();
        Self { font, font_size, cell_width, line_height }
    }

    fn dims_for(&self, bounds: Bounds<Pixels>) -> TermDims {
        TermDims {
            cols: ((bounds.size.width / self.cell_width).floor() as usize).max(2),
            rows: ((bounds.size.height / self.line_height).floor() as usize).max(1),
        }
    }
}

fn hsla(c: Rgb) -> Hsla {
    Rgba { r: c.r as f32 / 255.0, g: c.g as f32 / 255.0, b: c.b as f32 / 255.0, a: 1.0 }.into()
}

fn paint_grid(
    mut snap: GridSnapshot,
    m: &CellMetrics,
    bounds: Bounds<Pixels>,
    focused: bool,
    window: &mut Window,
    cx: &mut App,
) {
    window.paint_quad(fill(bounds, hsla(colors::BACKGROUND)));

    // A focused block cursor is drawn by inverting its cell, so the glyph stays readable.
    let mut outline_cursor = None;
    if let Some(c) = snap.cursor {
        if c.row < snap.rows && c.col < snap.cols {
            if focused && c.shape == CursorShape::Block {
                let cell = &mut snap.lines[c.row][c.col];
                cell.fg = colors::BACKGROUND;
                cell.bg = colors::FOREGROUND;
            } else {
                outline_cursor = Some(c);
            }
        }
    }

    let cell_origin = |row: usize, col: usize| {
        point(bounds.origin.x + m.cell_width * col as f32, bounds.origin.y + m.line_height * row as f32)
    };

    for (row, line) in snap.lines.iter().enumerate() {
        for rect in grid::bg_rects(line, colors::BACKGROUND) {
            let size = size(m.cell_width * rect.cells as f32, m.line_height);
            window.paint_quad(fill(Bounds::new(cell_origin(row, rect.col), size), hsla(rect.color)));
        }
        for span in grid::text_spans(line) {
            let color = hsla(span.fg);
            let run = TextRun {
                len: span.text.len(),
                font: Font {
                    weight: if span.bold { FontWeight::BOLD } else { FontWeight::NORMAL },
                    style: if span.italic { FontStyle::Italic } else { FontStyle::Normal },
                    ..m.font.clone()
                },
                color,
                background_color: None,
                underline: span.underline.then(|| UnderlineStyle {
                    color: Some(color),
                    thickness: px(1.0),
                    wavy: false,
                }),
                strikethrough: None,
            };
            // force_width = one cell per glyph keeps the text locked to the grid.
            let shaped = window.text_system().shape_line(
                SharedString::from(span.text),
                m.font_size,
                &[run],
                Some(m.cell_width),
            );
            let _ = shaped.paint(cell_origin(row, span.col), m.line_height, TextAlign::Left, None, window, cx);
        }
    }

    // Unfocused block = hollow box; beam = thin bar; underline = thin bottom bar.
    if let Some(c) = outline_cursor {
        let origin = cell_origin(c.row, c.col);
        let color = hsla(colors::FOREGROUND);
        let full = Bounds::new(origin, size(m.cell_width, m.line_height));
        match c.shape {
            CursorShape::Beam => window.paint_quad(fill(Bounds::new(origin, size(px(2.0), m.line_height)), color)),
            CursorShape::Underline => window.paint_quad(fill(
                Bounds::new(point(origin.x, origin.y + m.line_height - px(2.0)), size(m.cell_width, px(2.0))),
                color,
            )),
            _ => window.paint_quad(gpui::outline(full, color, gpui::BorderStyle::Solid)),
        }
    }
}
```

- [x] **Step 2: Replace `src/main.rs`** (M0's placeholder `Fuxx` view goes away):

```rust
// fuxx — a native GPU terminal for running AI coding agents.
// Milestone 1: one real terminal (alacritty_terminal + portable-pty) in a GPUI window.

mod colors;
mod grid;
mod keys;
mod pty;
mod terminal;
mod terminal_view;

use gpui::{
    App, Bounds, KeyBinding, QuitMode, TitlebarOptions, WindowBounds, WindowOptions, actions, px,
    size,
};
use gpui_platform::application;

use terminal_view::TerminalView;

actions!(fuxx, [Quit]);

fn main() {
    // macOS apps normally keep running with no windows; a terminal should quit instead.
    application().with_quit_mode(QuitMode::LastWindowClosed).run(|cx: &mut App| {
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());

        let bounds = Bounds::centered(None, size(px(900.0), px(560.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions { title: Some("fuxx".into()), ..Default::default() }),
                ..Default::default()
            },
            |window, cx| TerminalView::spawn(window, cx).expect("failed to start the shell"),
        )
        .expect("failed to open the window");
        cx.activate(true);
    });
}
```

- [x] **Step 3: Build with zero warnings:**

```bash
cargo build 2>&1 | grep -E "^(warning|error)" | grep -v "future version"
```
Expected: **no output**.

- [x] **Step 4: All tests:** `cargo test`. Expected: `test result: ok. 21 passed`.

- [x] **Step 5: Commit** *(when you say so)*: `git add src/terminal_view.rs src/main.rs && git commit -m "feat(m1): render a live terminal in the GPUI window"`

### Task 6: Use it for real — 👀 ✅

Run `cargo run` and check each item. Report failures with a screenshot plus the terminal output, then
run `superpowers:systematic-debugging` before changing code.

- [x] Window opens with **your shell prompt** (zsh), and the cursor is a solid block.
- [x] Typing shows characters; **enter** runs commands; **backspace** deletes.
- [x] `ls --color` (or `ls -G`) shows **colored** names.
- [x] `echo $TERM` prints `xterm-256color`.
- [x] `vim` opens **instantly** (no hang). Arrow keys move, `i` inserts, `esc` and `:q` quit, and the screen restores.
- [x] **ctrl-c** interrupts `sleep 100`. **ctrl-d** on an empty prompt exits.
- [x] **Resize** the window: the prompt re-wraps, and `tput cols; tput lines` matches the new size.
- [x] `echo 界🙂x`: the wide glyphs don't overlap the `x`.
- [x] `seq 1 200000`: it scrolls quickly, ends at `200000`, and the app stays responsive.
- [x] `claude` (Claude Code): its UI draws, updates smoothly, and takes input. *This exercises query replies and synchronized updates.*
- [x] Type `exit`: **the app quits** (one window, one shell).
- [x] **cmd-q** quits.

**If rendering is fundamentally unworkable** (the milestone note's fallback): stop and discuss
`gpui-ghostty` (needs Zig) before going further. That's a plan change ([[30 - Tech Stack]]).

### Task 7: Update the docs — ✅

- [x] [[41 - Milestone 1 - One Terminal Pane]]: tick the checklist; **Status:: ✅ Done**; record D2 (GPUI text) as decided.
- [x] [[30 - Tech Stack]]: the "Terminal rendering" section says GPUI text system (D2); add `futures` (D3) and the exact `alacritty_terminal`/`portable-pty` versions.
- [x] [[40 - Roadmap]]: M1 → ✅.
- [x] [[00 - Plan Tracker (Start Here)]]: M1 → ✅ 7/7; **Where we are** → M2 Task 1.
- [x] [[60 - Progress Log]]: new entry.
- [x] Commit docs *(when you say so)*.

## Verification log

| Date | Check | Result | Evidence |
|---|---|---|---|
| 2026-10-12 | Plan code compiles at pinned rev | ✅ 0 warnings | AI scratch build |
| 2026-10-12 | Plan unit tests | ✅ 21 passed | AI scratch `cargo test` |
| 2026-10-26 | Repo build, pinned rev | ✅ 0 warnings from our code | AI build output |
| 2026-10-26 | `cargo test` on `4a359c3` | ✅ 21 passed | AI test output |
| 2026-10-26 | Task 6 checklist (all 12 items) | ✅ | 👀 confirmed by you |

## Known limitations (deliberately not in M1, and where they land)

| Limitation | Lands in |
|---|---|
| No scrollback scrolling (history is kept by `Term`, but there's no scroll wheel / keys) | M5 Terminal UX |
| No selection, copy/paste, mouse reporting, clickable URLs | M5 |
| Font fixed to Menlo 14 px; no ligature/config | M6 Fonts & config |
| No IME / dead keys (printable text comes from `key_char`) | Not yet planned (needs GPUI `InputHandler`) |
| Cursor doesn't blink; bold isn't brightened | Polish (M5/M6) |
| Pixel cell size reported as 0 in `CSI 14 t` replies | Fine for agents; revisit if a program needs it |
| Shell exit quits the whole app | Changes in M3 (closes just that session) |

## Related

[[00 - Plan Tracker (Start Here)]] · [[M0 Plan - Scaffold and GPUI Window]] · [[M2 Plan - Agent State Detection]] · [[31 - API Fact Sheet (pinned rev)]]
