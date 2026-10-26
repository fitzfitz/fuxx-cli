---
tags: [development, plan, milestone]
milestone: M5
status: not-started
updated: 2026-10-26
---

# M5 Plan — Terminal UX (scrollback, selection, copy/paste, mouse, links, resize)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[45 - Milestone 5 - Terminal UX]] (the spec: every behaviour below is listed there) |
| **Status** | ⬜ Not started |
| **Tasks done** | 0 / 8 |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | The behaviour calls in [[45 - Milestone 5 - Terminal UX]] (made by the AI, modelled on Zed; you said "go") |
| **Needs your eyes** | 👀 Task 7: scroll, select, copy/paste, mouse apps, links, resize, on screen |

**Goal:** The terminal pane scrolls back through history, selects/copies/pastes text, hands the
mouse to programs that ask for it, opens links with cmd-click, and stays correct on resize.

**Architecture:**

```
 GPUI events (terminal_view.rs)                 pure logic (unit-tested)                 TerminalCore (terminal.rs)
 scroll wheel ──► ScrollAccumulator (mouse.rs) ─► lines ─► report / alt_scroll / scroll ─► core.scroll(Scroll::Delta)
 mouse down/move/up ─► grid_point (mouse.rs) ──► (Point, Side) ─┬► report (mouse.rs) ───► core.write
                                                                ├► start/extend_selection ─► Term.selection
                                                                └► link_at (links.rs) ───► cx.open_url (http/https/mailto)
 cmd-c / cmd-v ─► Copy / Paste actions ──────────► selection_text / paste_bytes (keys.rs) ─► clipboard / core.input
 keystroke ─► core.input = scroll to bottom + clear selection + write
 prepaint ─► CellMetrics ─► core.resize(dims, CellPx) ─► Term + PTY (real pixel size)
 snapshot (grid.rs): selected cells get colors::SELECTION; hovered link cells underlined
 status tick ─► core.tail_text (live screen, ignores scrollback)
```

**Tech Stack:** No new crates. `alacritty_terminal 0.26.0` (`grid::Scroll`, `selection`, `term::search`,
`index`), GPUI at the pinned rev (mouse events, clipboard, `open_url`, hitbox cursor style).

**Spec:** [[45 - Milestone 5 - Terminal UX]] · [[40 - Roadmap]] (M5) · [[11 - Features]] (A) · [[12 - Scope]]

## Global Constraints

- Everything from the M0–M4b plans. **No new crates.** 0 warnings from our code. Final count **83 tests**.
- Behaviour is exactly the list in [[45 - Milestone 5 - Terminal UX]]; don't add anything it puts under "Not in M5".
- Only `core.input` (keystrokes, paste) snaps to the bottom and clears the selection. Replies to terminal
  queries still go through `core.write` and must **not** move the view.
- Links open **only** for `http://`, `https://`, `mailto:`, plain-text or OSC 8, and only on **cmd-click**.
- Pure logic lives outside GPUI (`mouse.rs`, `links.rs`, `grid.rs`, `keys.rs`) so it is unit-tested;
  `terminal_view.rs` only wires events to it.
- Selection color: `colors::SELECTION = hex(0x264f78)`.
- Never hold the `Term` lock while calling back into GPUI (as before).

## Review Focus

1. **Status detection breaks while you're scrolled up** (a waiting agent shows Working) → `screen_tail_ignores_scrollback` (Task 1).
2. **A terminal query reply jumps the view to the bottom** while you read history → `scrolling_moves_the_view_and_input_snaps_back` asserts `write` keeps the offset (Task 1).
3. **Pasted text runs commands or escapes the paste** (embedded ESC / `\x1b[201~`) → `bracketed_paste_wraps_and_strips_escape` (Task 2).
4. **Program output makes cmd-click launch a local file or app** (`file://`, custom schemes via OSC 8) → `disallowed_schemes_never_open` (Task 4).
5. **Mouse reports with wrong coordinates once scrolled or near the edge** (clicks land on the wrong cell, vim/htop misbehave) → `grid_point_accounts_for_scrollback`, `normal_encoding_and_limit` (Task 3); Task 7 checks vim/htop.

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `src/grid.rs` | Modify | `screen_tail_text`; selection background in `snapshot`; `underline_cells` | 1, 2, 4 |
| `src/terminal.rs` | Modify | `input`, `scroll`, `display_offset`, `tail_text`, selection API, `link_at`, `CellPx` resize, `window_size` | 1, 2, 4, 5 |
| `src/colors.rs` | Modify | `SELECTION` | 2 |
| `src/keys.rs` | Modify | `paste_bytes` | 2 |
| `src/mouse.rs` | Create | `grid_point`, `ScrollAccumulator`, `report`, `alt_scroll`, `wants_mouse` | 3 |
| `src/links.rs` | Create | `LinkFinder`, `Link`, `allowed` | 4 |
| `src/pty.rs` | Modify | `pty_size(dims, CellPx)` with pixels | 5 |
| `src/terminal_view.rs` | Modify | Wire wheel, mouse, actions, hover, cursor style, paint | 1, 5, 6 |
| `src/main.rs` | Modify | `mod mouse; mod links;`, Terminal-context key bindings | 3, 4, 6 |

---

### Task 1: Scrollback core + live-screen status — ⬜

**Files:** Modify `src/grid.rs`, `src/terminal.rs`, `src/terminal_view.rs` (two call sites).

**Interfaces:** Produces in `terminal.rs`:
`TerminalCore::input(&self, bytes: &[u8])` (scroll to bottom, clear selection, then `write`),
`TerminalCore::scroll(&self, scroll: alacritty_terminal::grid::Scroll)`,
`TerminalCore::display_offset(&self) -> usize`, `TerminalCore::tail_text(&self, n: usize) -> String`.
In `grid.rs`: `pub fn screen_tail_text<L: EventListener>(term: &Term<L>, n: usize) -> String`,
replacing `GridSnapshot::tail_text` (removed).

> **Why the tail moves.** `snapshot` copies the *visible* rows, which are history rows when you're
> scrolled up. The status check must read the live screen: `term.grid()[Line(r)]` for
> `r in 0..screen_lines` is always the live screen, whatever the display offset.

- [ ] **Step 1: Failing tests.** In `grid.rs` tests: change `tail_text_returns_last_non_blank_lines` to call
  `screen_tail_text(&term, n)` (same assertions), and add:

```rust
#[test]
fn screen_tail_ignores_scrollback() {
    let text: String = (1..=30).map(|i| format!("l{i}\r\n")).collect();
    let mut term = Term::new(Config::default(), &TermDims { cols: 10, rows: 5 }, VoidListener);
    let mut p: Processor<StdSyncHandler> = Processor::new();
    p.advance(&mut term, text.as_bytes());
    term.scroll_display(Scroll::Delta(3));
    // The view shows history (l24 at the top)...
    assert_eq!(snapshot(&term).lines[0].iter().map(|c| c.ch).collect::<String>().trim_end(), "l24");
    // ...but the status tail still reads the live bottom.
    assert_eq!(screen_tail_text(&term, 1), "l30");
}
```

  In `terminal.rs` tests:

```rust
#[test]
fn scrolling_moves_the_view_and_input_snaps_back() {
    let (tx, mut rx) = unbounded();
    let core = TerminalCore::spawn(
        sh("for i in $(seq 1 40); do echo line$i; done; sleep 2"),
        TermDims { cols: 20, rows: 5 },
        tx,
    )
    .unwrap();
    assert!(wait_for(&mut rx, |_| screen_text(&core).contains("line40")));
    core.scroll(Scroll::Delta(5));
    assert_eq!(core.display_offset(), 5);
    core.write(b"x"); // a query reply must not move the view
    assert_eq!(core.display_offset(), 5);
    core.input(b"x"); // typing does
    assert_eq!(core.display_offset(), 0);
}
```

- [ ] **Step 2: Run** `cargo test`. Expected: compile errors (`screen_tail_text`, `scroll`, `display_offset`, `input` not found).
- [ ] **Step 3: Implement** `screen_tail_text` (same trimming/blank-skipping as the old `tail_text`, reading
  `term.grid()[Line(r)][Column(c)].c`), remove `GridSnapshot::tail_text`, and the four `TerminalCore`
  methods (`scroll` → `term.scroll_display`; `display_offset` → `term.grid().display_offset()`;
  `input` → `scroll_display(Scroll::Bottom)`, `term.selection = None`, then `write`). In `terminal_view.rs`,
  `on_tick` passes `|| core.tail_text(TAIL_LINES)` and `on_key_down` calls `self.core.input(&bytes)`.
- [ ] **Step 4: Run** `cargo test`. Expected: `test result: ok. 63 passed`.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m5): scrollback core; status reads the live screen"`

### Task 2: Selection, copy text, paste bytes — ⬜

**Files:** Modify `src/colors.rs`, `src/grid.rs`, `src/terminal.rs`, `src/keys.rs`.

**Interfaces:** Produces `colors::SELECTION: Rgb` (`hex(0x264f78)`); in `terminal.rs`:
`start_selection(&self, ty: SelectionType, point: Point, side: Side)`,
`extend_selection(&self, point: Point, side: Side)` (no-op without a selection),
`clear_selection(&self)`, `selection_text(&self) -> Option<String>`; in `keys.rs`:
`pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8>`. `snapshot` paints selected cells with
`SELECTION` as their background (via `renderable_content().selection` and `SelectionRange::contains(indexed.point)`).
Types: `alacritty_terminal::selection::{Selection, SelectionType}`, `alacritty_terminal::index::{Point, Line, Column, Side}`
(`Side` is an alias of `Direction::{Left, Right}`, `index.rs:14`).

- [ ] **Step 1: Failing tests.**

```rust
// grid.rs
#[test]
fn selected_cells_get_the_selection_background() {
    let mut term = term_with(b"hello");
    let mut sel = Selection::new(SelectionType::Simple, Point::new(Line(0), Column(0)), Side::Left);
    sel.update(Point::new(Line(0), Column(1)), Side::Right);
    term.selection = Some(sel);
    let snap = snapshot(&term);
    assert_eq!(snap.lines[0][0].bg, colors::SELECTION);
    assert_eq!(snap.lines[0][1].bg, colors::SELECTION);
    assert_eq!(snap.lines[0][2].bg, colors::BACKGROUND);
}

// terminal.rs
#[test]
fn selection_text_word_and_input_clears_it() {
    let (tx, mut rx) = unbounded();
    let core = TerminalCore::spawn(sh("printf 'hello world'; sleep 2"), TermDims { cols: 20, rows: 3 }, tx).unwrap();
    assert!(wait_for(&mut rx, |_| screen_text(&core).contains("world")));
    core.start_selection(SelectionType::Simple, Point::new(Line(0), Column(0)), Side::Left);
    core.extend_selection(Point::new(Line(0), Column(4)), Side::Right);
    assert_eq!(core.selection_text().as_deref(), Some("hello"));
    core.start_selection(SelectionType::Semantic, Point::new(Line(0), Column(7)), Side::Left);
    assert_eq!(core.selection_text().as_deref(), Some("world"));
    core.input(b"x");
    assert_eq!(core.selection_text(), None);
}

// keys.rs
#[test]
fn bracketed_paste_wraps_and_strips_escape() {
    assert_eq!(paste_bytes("a\x1b[201~b\nc", true), b"\x1b[200~a[201~b\nc\x1b[201~".to_vec());
}

#[test]
fn plain_paste_turns_newlines_into_enter() {
    assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc".to_vec());
}
```

- [ ] **Step 2: Run** `cargo test`. Expected: compile errors for the new items.
- [ ] **Step 3: Implement.** Selection methods lock the term and set/update/clear `term.selection`
  (`Selection::new`, `Selection::update`); `selection_text` = `term.selection_to_string()`.
  `paste_bytes`: bracketed → `ESC[200~` + text with every `\x1b` removed + `ESC[201~`; plain →
  `\r\n` and `\n` become `\r`. `snapshot`: if the cell's grid point is in the selection range,
  `bg = colors::SELECTION` (after the inverse swap, so the selection always shows).
- [ ] **Step 4: Run** `cargo test`. Expected: `test result: ok. 67 passed`.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m5): selection, copy text and paste bytes"`

### Task 3: `mouse.rs` — positions, wheel, mouse reports — ⬜

**Files:** Create `src/mouse.rs` · Modify `src/main.rs` (`mod mouse;`).

**Interfaces:** Produces (no GPUI types):
- `pub fn grid_point(x: f32, y: f32, cell_width: f32, line_height: f32, dims: TermDims, display_offset: usize) -> (Point, Side)`
  — `x, y` relative to the grid's top-left. Column = `x / cell_width` clamped to `cols-1` (past the
  right edge → `Side::Right`); side = `Right` if the x offset inside the cell is **>** half a cell;
  row = `y / line_height` clamped to `rows-1` (below → `Side::Right`), negative y → `Side::Left`;
  result `Point::new(Line(row - display_offset), Column(col))` (Zed's `grid_point_and_side`, `mappings/mouse.rs:202`).
- `#[derive(Default)] pub struct ScrollAccumulator` with `reset(&mut self)` and
  `push(&mut self, delta_px: f32, line_height: f32) -> i32`: adds to a running pixel total and returns
  `trunc(new_total / lh) - trunc(old_total / lh)`. Positive = scroll up into history (GPUI's sign).
- `pub enum Button { Left, Middle, Right, WheelUp, WheelDown }`, `pub enum MouseAction { Press, Release, Drag }`,
  `#[derive(Default)] pub struct Mods { pub shift: bool, pub alt: bool, pub ctrl: bool }` (all `Clone, Copy, Debug, PartialEq, Eq`).
- `pub fn report(button: Button, action: MouseAction, mods: Mods, col: usize, row: usize, mode: TermMode) -> Option<Vec<u8>>`
  — `row` is the **viewport** row. `None` unless `mode.intersects(TermMode::MOUSE_MODE)`; `Drag` needs
  `MOUSE_DRAG | MOUSE_MOTION`; wheel `Release` → `None`. Codes: Left 0, Middle 1, Right 2, WheelUp 64,
  WheelDown 65; `Drag` +32; shift +4, alt +8, ctrl +16. **SGR** (`SGR_MOUSE`): `ESC [ < code ; col+1 ; row+1`
  then `M` (press/drag) or `m` (release, keeps the button code). **Normal**: `ESC [ M`, `32+code`
  (release uses code 3 + mods), then col and row each as `33 + pos`: one byte if `pos < 223`, else `None`;
  with `UTF8_MOUSE` the value `33 + pos` is UTF-8-encoded (`pos < 2015`, else `None`).
- `pub fn alt_scroll(lines: i32) -> Vec<u8>` — `|lines|` times `ESC O A` (lines > 0) or `ESC O B`.
- `pub fn wants_mouse(mode: TermMode, shift: bool) -> bool` — `mode.intersects(MOUSE_MODE) && !shift`.

- [ ] **Step 1: Failing tests** in `src/mouse.rs` (`use alacritty_terminal::term::TermMode;`):

```rust
const D: TermDims = TermDims { cols: 80, rows: 24 };
const SGR: TermMode = TermMode::MOUSE_REPORT_CLICK.union(TermMode::SGR_MOUSE);

#[test]
fn grid_point_clamps_and_picks_side() {
    assert_eq!(grid_point(25.0, 45.0, 10.0, 20.0, D, 0), (Point::new(Line(2), Column(2)), Side::Left));
    assert_eq!(grid_point(29.0, 45.0, 10.0, 20.0, D, 0), (Point::new(Line(2), Column(2)), Side::Right));
    assert_eq!(grid_point(5000.0, 9000.0, 10.0, 20.0, D, 0), (Point::new(Line(23), Column(79)), Side::Right));
}

#[test]
fn grid_point_accounts_for_scrollback() {
    assert_eq!(grid_point(0.0, 45.0, 10.0, 20.0, D, 3).0, Point::new(Line(-1), Column(0)));
}

#[test]
fn scroll_accumulator_keeps_fractions() {
    let mut acc = ScrollAccumulator::default();
    assert_eq!([acc.push(10.0, 20.0), acc.push(15.0, 20.0), acc.push(-30.0, 20.0)], [0, 1, -1]);
}

#[test]
fn sgr_press_release_drag_and_wheel() {
    let m = Mods::default();
    assert_eq!(report(Button::Left, MouseAction::Press, m, 0, 0, SGR).unwrap(), b"\x1b[<0;1;1M");
    assert_eq!(report(Button::Left, MouseAction::Release, m, 0, 0, SGR).unwrap(), b"\x1b[<0;1;1m");
    let drag = SGR.union(TermMode::MOUSE_DRAG);
    assert_eq!(report(Button::Left, MouseAction::Drag, m, 4, 2, drag).unwrap(), b"\x1b[<32;5;3M");
    assert_eq!(report(Button::WheelUp, MouseAction::Press, m, 0, 0, SGR).unwrap(), b"\x1b[<64;1;1M");
}

#[test]
fn modifiers_add_bits() {
    let ctrl = Mods { ctrl: true, ..Mods::default() };
    assert_eq!(report(Button::Right, MouseAction::Press, ctrl, 0, 0, SGR).unwrap(), b"\x1b[<18;1;1M");
}

#[test]
fn normal_encoding_and_limit() {
    let normal = TermMode::MOUSE_REPORT_CLICK;
    let m = Mods::default();
    assert_eq!(report(Button::Left, MouseAction::Press, m, 0, 0, normal).unwrap(), vec![0x1b, b'[', b'M', 32, 33, 33]);
    assert_eq!(report(Button::Left, MouseAction::Release, m, 1, 2, normal).unwrap(), vec![0x1b, b'[', b'M', 35, 34, 35]);
    assert_eq!(report(Button::Left, MouseAction::Press, m, 223, 0, normal), None);
    let utf8 = normal.union(TermMode::UTF8_MOUSE);
    assert_eq!(report(Button::Left, MouseAction::Press, m, 223, 0, utf8).unwrap(), [b"\x1b[M ".as_slice(), "\u{100}".as_bytes(), b"!"].concat());
}

#[test]
fn no_report_without_mouse_mode_and_drag_rules() {
    let m = Mods::default();
    assert_eq!(report(Button::Left, MouseAction::Press, m, 0, 0, TermMode::empty()), None);
    assert_eq!(report(Button::Left, MouseAction::Drag, m, 0, 0, SGR), None); // click-only mode
    assert_eq!(report(Button::WheelUp, MouseAction::Release, m, 0, 0, SGR), None);
    assert!(wants_mouse(SGR, false) && !wants_mouse(SGR, true));
}

#[test]
fn alt_scroll_sends_ss3_arrows() {
    assert_eq!(alt_scroll(2), b"\x1bOA\x1bOA".to_vec());
    assert_eq!(alt_scroll(-1), b"\x1bOB".to_vec());
}
```

- [ ] **Step 2: Run** `cargo test mouse`. Expected: compile errors (items not defined).
- [ ] **Step 3: Implement** the functions above in `src/mouse.rs`; add `mod mouse;` to `main.rs`.
- [ ] **Step 4: Run** `cargo test`. Expected: `test result: ok. 75 passed` (dead-code warnings until Task 6).
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m5): mouse positions, wheel accumulator and mouse reports"`

### Task 4: `links.rs` — finding links, safely — ⬜

**Files:** Create `src/links.rs` · Modify `src/terminal.rs`, `src/grid.rs`, `src/main.rs` (`mod links;`).

**Interfaces:** Produces:
- `#[derive(Clone, Debug, PartialEq)] pub struct Link { pub url: String, pub start: Point, pub end: Point }`
- `pub fn allowed(url: &str) -> bool` — true only for `http://`, `https://`, `mailto:` (case-insensitive scheme).
- `pub struct LinkFinder { regex: RegexSearch }` with `new() -> Self` and
  `find<L: EventListener>(&mut self, term: &Term<L>, point: Point) -> Option<Link>`: first the cell's
  OSC 8 `hyperlink()` (extend left/right over cells with the same `uri()`, on the same line); else
  `RegexIter::new(term.line_search_left(point), term.line_search_right(point), Direction::Right, term, &mut self.regex)`
  `.find(|m| m.contains(&point))`, text via `term.bounds_to_string`, then trim trailing `. , : ; (` and
  unbalanced `)` (Zed's `sanitize_url_punctuation`, `alacritty/hyperlinks.rs:236`), shortening `end` to match.
  Returns `None` unless `allowed(&url)`. Regex (Zed's `URL_REGEX`, narrowed to the allowed schemes):
  ``(https://|http://|mailto:)[^\u{0000}-\u{001F}\u{007F}-\u{009F}<>"\s{-}\^⟨⟩`']+``
- `TerminalCore::link_at(&self, point: Point) -> Option<Link>` (holds an `Arc<Mutex<LinkFinder>>`; the regex is built once).
- `grid::underline_cells(snap: &mut GridSnapshot, start: Point, end: Point, display_offset: usize)` — sets
  `underline = true` on every visible cell from `start` to `end` inclusive (wrapping across lines).

- [ ] **Step 1: Failing tests** in `src/links.rs` (helper `term_with(bytes, cols)`: a `cols`×3 `Term` fed `bytes`, as in `grid.rs`), plus one in `grid.rs`:

```rust
#[test]
fn url_under_point_is_found_and_trimmed() {
    let term = term_with(b"see https://example.com/a). ok", 40);
    let link = LinkFinder::new().find(&term, Point::new(Line(0), Column(8))).unwrap();
    assert_eq!(link.url, "https://example.com/a");
    assert_eq!((link.start.column.0, link.end.column.0), (4, 24));
}

#[test]
fn osc8_link_is_found() {
    let term = term_with(b"\x1b]8;;https://x.dev\x1b\\link\x1b]8;;\x1b\\ plain", 40);
    let link = LinkFinder::new().find(&term, Point::new(Line(0), Column(1))).unwrap();
    assert_eq!((link.url.as_str(), link.start.column.0, link.end.column.0), ("https://x.dev", 0, 3));
}

#[test]
fn disallowed_schemes_never_open() {
    let term = term_with(b"\x1b]8;;file:///Applications/Calculator.app\x1b\\calc\x1b]8;;\x1b\\ file:///etc/passwd", 60);
    let mut finder = LinkFinder::new();
    assert_eq!(finder.find(&term, Point::new(Line(0), Column(1))), None);
    assert_eq!(finder.find(&term, Point::new(Line(0), Column(10))), None);
    assert!(allowed("HTTPS://a.b") && allowed("mailto:x@y.z") && !allowed("javascript:x") && !allowed("ssh://h"));
}

#[test]
fn plain_text_has_no_link() {
    assert_eq!(LinkFinder::new().find(&term_with(b"just words", 20), Point::new(Line(0), Column(2))), None);
}

// grid.rs
#[test]
fn underline_cells_marks_the_link() {
    let mut snap = snapshot(&term_with(b"ab https"));
    underline_cells(&mut snap, Point::new(Line(0), Column(3)), Point::new(Line(0), Column(7)), 0);
    assert!(!snap.lines[0][2].underline && snap.lines[0][3].underline && snap.lines[0][7].underline);
}
```

  *(`term_with` in `grid.rs` is 10×3, so `"ab https"` fits on one line.)*
- [ ] **Step 2: Run** `cargo test`. Expected: compile errors.
- [ ] **Step 3: Implement** as specified; add `mod links;`.
- [ ] **Step 4: Run** `cargo test`. Expected: `test result: ok. 80 passed`.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m5): link finder (http/https/mailto only, plain text and OSC 8)"`

### Task 5: Resize with real pixel sizes — ⬜

**Files:** Modify `src/pty.rs`, `src/terminal.rs`, `src/terminal_view.rs` (resize + `TextAreaSizeRequest` call sites).

**Interfaces:** Produces `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub struct CellPx { pub width: u16, pub height: u16 }`
in `terminal.rs`; `pub fn pty_size(dims: TermDims, cell: CellPx) -> PtySize` in `pty.rs` (`pixel_width = cols × width`,
`pixel_height = rows × height`); `Pty::resize(&self, dims, cell)`; `TerminalCore::resize(&self, dims: TermDims, cell: CellPx)`
(no-op when both are unchanged or dims are 0); `TerminalCore::window_size(&self) -> WindowSize`
(`num_lines`, `num_cols`, `cell_width`, `cell_height` from the last resize). The view passes
`CellPx { width: cell_width.round() as u16, height: line_height.round() as u16 }` and answers
`TextAreaSizeRequest` with `format(core.window_size())`. Update the existing
`resize_changes_the_grid_and_ignores_zero` to the new signature.

- [ ] **Step 1: Failing tests.**

```rust
// pty.rs
#[test]
fn pty_size_carries_pixels() {
    let s = pty_size(TermDims { cols: 80, rows: 24 }, CellPx { width: 8, height: 17 });
    assert_eq!((s.cols, s.rows, s.pixel_width, s.pixel_height), (80, 24, 640, 408));
}

// terminal.rs
#[test]
fn csi_14t_reports_real_pixel_size() {
    let (tx, mut rx) = unbounded();
    let core = TerminalCore::spawn(sh("sleep 1; printf '\\033[14t'; sleep 1"), TermDims { cols: 80, rows: 24 }, tx).unwrap();
    core.resize(TermDims { cols: 80, rows: 24 }, CellPx { width: 8, height: 17 });
    let mut reply = None;
    assert!(wait_for(&mut rx, |e| match e {
        TermEvent::Term(Event::TextAreaSizeRequest(format)) => { reply = Some(format(core.window_size())); true }
        _ => false,
    }));
    assert_eq!(reply.as_deref(), Some("\x1b[4;408;640t"));
}

#[test]
fn width_change_clears_selection() {
    let (tx, mut rx) = unbounded();
    let core = TerminalCore::spawn(sh("printf 'hello'; sleep 2"), TermDims { cols: 20, rows: 3 }, tx).unwrap();
    assert!(wait_for(&mut rx, |_| screen_text(&core).contains("hello")));
    core.start_selection(SelectionType::Simple, Point::new(Line(0), Column(0)), Side::Left);
    core.extend_selection(Point::new(Line(0), Column(4)), Side::Right);
    core.resize(TermDims { cols: 15, rows: 3 }, CellPx { width: 8, height: 17 });
    assert_eq!(core.selection_text(), None);
}
```

  *(`wait_for` takes `FnMut`, so the closure may write `reply`.)*
- [ ] **Step 2: Run** `cargo test`. Expected: compile errors (`CellPx`, two-argument `resize`, `window_size`).
- [ ] **Step 3: Implement** as specified (the dims+cell "unchanged" check replaces today's dims-only check).
- [ ] **Step 4: Run** `cargo test`. Expected: `test result: ok. 83 passed`.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m5): resize reports real pixel sizes to the PTY and CSI 14 t"`

### Task 6: Wire it into the view — ⬜

**Files:** Modify `src/terminal_view.rs`, `src/main.rs`.

**Interfaces:** Consumes everything above. Produces `actions!(terminal, [Copy, Paste, ScrollPageUp, ScrollPageDown])`
in `terminal_view.rs`, bound in `main.rs` with context `Some("Terminal")`: `cmd-c` Copy, `cmd-v` Paste,
`shift-pageup` / `cmd-up` ScrollPageUp, `shift-pagedown` / `cmd-down` ScrollPageDown.
(GPUI dispatches key bindings **before** `on_key_down`, `window.rs:4905–4960`, so these win over the byte mapping.)

> **No unit tests here:** this is GPUI event plumbing. Every decision it makes is a call into the
> tested functions of Tasks 1–5. It's verified by a clean build here and on screen in Task 7.

- [ ] **Step 1: Layout for hit-testing.** `TerminalView` gets `layout: Rc<Cell<Option<GridLayout>>>`
  (`GridLayout { origin: Point<Pixels>, cell_width: Pixels, line_height: Pixels, dims: TermDims }`),
  written in the canvas prepaint (which also returns `window.insert_hitbox(bounds, HitboxBehavior::Normal)`
  alongside the metrics), plus `scroll: ScrollAccumulator`, `hovered: Option<Link>`, `pressed_link: Option<Link>`,
  `selecting: bool`. A helper `point_at(&self, pos: Point<Pixels>) -> Option<(Point, Side, usize, usize)>`
  returns the grid point, side and viewport col/row via `mouse::grid_point` (position minus `origin`).
- [ ] **Step 2: Wheel** (`div().on_scroll_wheel`): `TouchPhase::Started` → `scroll.reset()`; lines =
  `scroll.push(e.delta.pixel_delta(line_height).y, line_height)`; 0 → return. Then, in order:
  `mouse::wants_mouse(mode, shift)` → one `report(WheelUp/WheelDown, Press, …)` per line;
  else `ALT_SCREEN` and `ALTERNATE_SCROLL` set and not shift → `core.write(&alt_scroll(lines))`;
  else `core.scroll(Scroll::Delta(lines))`. `cx.notify()`.
- [ ] **Step 3: Mouse down** (`div().on_mouse_down` for Left, Middle, Right): focus the view. Left + cmd →
  `pressed_link = core.link_at(point)`; if some, stop. Else `wants_mouse` → `report(button, Press, …)`.
  Else Left only: `shift` → `extend_selection`; else by `click_count` 1/2/3 → `start_selection(Simple/Semantic/Lines)`;
  `selecting = true`. `cx.notify()`.
- [ ] **Step 4: Move and up anywhere in the window.** In the canvas paint closure, register
  `window.on_mouse_event::<MouseMoveEvent>` and `::<MouseUpEvent>` (Bubble phase only), updating the view
  through a captured `Entity<TerminalView>` (so a drag that leaves the pane keeps selecting). Move:
  `selecting` with Left held → `extend_selection`; `wants_mouse` with a button held → `report(.., Drag, ..)`
  (positions clamped by `grid_point`); cmd held → `hovered = core.link_at(point)`, else `hovered = None`
  (notify only on change). Up: `pressed_link` is `Some` and equals `core.link_at(point)` → `cx.open_url(&link.url)`,
  then `pressed_link = None`; `wants_mouse` → `report(button, Release, ..)`; `selecting = false`.
- [ ] **Step 5: Modifier changes** (`div().on_modifiers_changed`): cmd released → `hovered = None`, notify.
- [ ] **Step 6: Actions** (`div().on_action`): Copy → `selection_text()` → `cx.write_to_clipboard(ClipboardItem::new_string(t))`
  (selection kept). Paste → `cx.read_from_clipboard()` → `.text()` → `core.input(&keys::paste_bytes(&t, mode.contains(BRACKETED_PASTE)))`,
  then the same status `on_input` as a keystroke. ScrollPageUp/Down → in `ALT_SCREEN`, `cx.propagate()`
  (the program gets the key); else `core.scroll(Scroll::PageUp/PageDown)` and notify.
- [ ] **Step 7: Paint.** After `snapshot()`, if `hovered` is set, `grid::underline_cells(&mut snap, link.start, link.end, core.display_offset())`.
  Cursor: `PointingHand` when `hovered` is set and cmd is held, else `IBeam` (`window.set_cursor_style(style, &hitbox)`).
- [ ] **Step 8: Build with zero warnings:** `cargo build 2>&1 | grep -E "^(warning|error)" | grep -v "future version"`. Expected: **no output**. `cargo test`: `83 passed`.
- [ ] **Step 9: Commit:** `git add src && git commit -m "feat(m5): wheel, selection, copy/paste, mouse reporting and links in the view"`

### Task 7: On screen — 👀 ⬜

Run `cargo run`. **You** check and report:

- [ ] **Scroll:** `seq 1 3000`, then wheel/trackpad up and down; shift-PageUp/PageDown and cmd-↑/↓ page. While scrolled up, `sleep 3; echo hi` doesn't pull you down; typing a key does.
- [ ] **Status while scrolled:** run `bash -c 'sleep 3; printf "Proceed? (y/n) "; read x'` after some output, scroll up while it waits → the ring still turns 🔴.
- [ ] **Select:** drag across text (blue background); double-click a word; triple-click a line; shift-click extends. Dragging below the pane keeps selecting.
- [ ] **Copy/paste:** cmd-c, then cmd-v into another session (cmd-t). A multi-line paste at the zsh prompt doesn't run each line (bracketed paste). Typing clears the selection.
- [ ] **Mouse apps:** `vim` with `:set mouse=a` → clicking moves the cursor, the wheel scrolls the file. `htop` → clicks work. Shift-drag still selects text locally. `man ls` → the wheel scrolls (arrows).
- [ ] **Links:** `echo https://example.com`; hold cmd and hover → underline + hand; cmd-click opens the browser; a plain click doesn't. `printf '\e]8;;https://example.com\e\\click me\e]8;;\e\\\n'` → cmd-click opens it.
- [ ] **Resize:** drag the window edge smaller/bigger while `htop` runs and while a long line is on screen → no garbage; text reflows.
- [ ] **Agents:** `claude` works as before; scroll back through its output; status rings unchanged.

### Task 8: Update the docs — ⬜

- [ ] [[45 - Milestone 5 - Terminal UX]]: tick the checklist; **Status:: ✅ Done**.
- [ ] [[40 - Roadmap]]: M5 → ✅ (link the note).
- [ ] [[00 - Plan Tracker (Start Here)]]: M5 row → ✅ 8/8; **Where we are** → M6 (fonts & config, incl. prompt icons), plan to be written.
- [ ] [[31 - API Fact Sheet (pinned rev)]]: add the alacritty `Scroll` / `selection` / `search` and GPUI mouse/clipboard facts this plan relied on.
- [ ] `STRUCTURE.md`: `mouse.rs`, `links.rs` (M5) and the M5 additions to `grid.rs`, `terminal.rs`, `terminal_view.rs`.
- [ ] [[60 - Progress Log]]: entry "M5 complete: terminal UX".
- [ ] Commit docs.

## Verification log

| Date | Check | Result | Evidence |
|---|---|---|---|
| 2026-10-26 | APIs read at the pinned rev / alacritty 0.26.0 (scroll, selection, search, OSC 8, GPUI mouse events, clipboard, `open_url`, hitbox cursor, key dispatch order) | ✅ | Research of Zed's `crates/terminal*` + alacritty sources, with file:line refs |
| 2026-10-26 | Spike in a scratch copy: `scroll_display` + live `Line(0..)` tail, `Selection` Simple/Semantic/Lines + `selection_to_string`, `RegexIter` URL match, OSC 8 `hyperlink().uri()` | ✅ 3 spike tests (after correcting one expectation: the view's top row is `l24`) | AI `cargo test spike` |
| — | Task code itself | ⚠️ not pre-compiled: this plan gives signatures and tests; Task 6's GPUI wiring is unverified until built | Verified per task during execution |

## Known limitations

| Limitation | Lands in |
|---|---|
| No search in scrollback | P2 ([[11 - Features]]) |
| File paths aren't clickable (URLs only) | Later (the roadmap's M5 says URLs) |
| Dragging a selection past the top/bottom edge doesn't auto-scroll | Polish, if wanted |
| No copy-on-select, no right-click menu | Not planned |
| Mouse motion without a button held isn't reported (hover effects in TUIs) | Revisit if a program needs it |
| Prompt icons still show as `?` | M6 |

## Related

[[00 - Plan Tracker (Start Here)]] · [[M4b Plan - Tester Build]] · [[40 - Roadmap]] · [[31 - API Fact Sheet (pinned rev)]]
