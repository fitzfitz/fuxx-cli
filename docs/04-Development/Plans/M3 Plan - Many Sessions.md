---
tags: [development, plan, milestone]
milestone: M3
status: not-started
updated: 2026-10-12
---

# M3 Plan — Many Sessions

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[43 - Milestone 3 - Many Sessions]] |
| **Status** | ⬜ Not started (waits for M2 ✅) |
| **Tasks done** | 0 / 5 |
| **▶ Pick up here** | **Task 1, Step 1** |
| **Decisions used** | ❓ **D6** = keyboard shortcuts (see [[00 - Plan Tracker (Start Here)]]) |
| **Needs your eyes** | 👀 Task 4 (3 sessions, switching, background keeps running) |

**Goal:** Many sessions in one window. A sidebar lists them; click or a shortcut switches the main
pane; every session keeps running in the background; a session closes when its shell exits.

**Architecture:**

```
 Workspace (root view)
 ├─ sidebar: one row per Session → click = select(i)
 ├─ main pane: only the ACTIVE TerminalView is rendered
 └─ SessionList<Session>  (pure, unit-tested: push / select / next / prev / remove)
       Session { id, view: Entity<TerminalView>, title, _subscription }
                        │ emits TerminalEvent::{TitleChanged, Exited}
                        └──────────► Workspace::on_terminal_event (subscribe_in)
```

- Background sessions keep running because their `TerminalView` entities stay alive in the list.
  Each one owns its reader thread, event task and ticker, so it doesn't matter that they aren't drawn.
- Switching = `SessionList::select(i)` + focus the new view's `FocusHandle`.
- Closing (cmd-w or shell exit) drops the `Session` → view → `TerminalCore` → `Pty`, whose `Drop` kills the child.
  Closing the last session closes the window, and `QuitMode::LastWindowClosed` quits the app.

**Tech Stack:** No new dependencies.

**Spec:** [[43 - Milestone 3 - Many Sessions]] · [[11 - Features]] (section B) · [[20 - Architecture Overview]]

## Global Constraints

- Everything from the M0–M2 plans.
- **No new dependencies.** No split panes (deferred, [[12 - Scope]]). No agent presets, no rename/reorder (Phase 2, M7).
- New sessions start the user's login shell in `$HOME` (agent presets come in Phase 2).
- Session label = the title the program set (OSC 0/2), else `Shell <id>`.
- Shortcuts (D6): `cmd-t` new · `cmd-w` close (no confirmation) · `cmd-1…9` jump · `cmd-{` / `cmd-}` previous/next · `cmd-q` quit.
- The status is still only logged (rings are M4).

## Review Focus

1. **Closing the active session must land focus on a sensible neighbour** → `SessionList::remove` tests (Task 1): the next one slides in, or the previous one when the last was closed.
2. **Closing the last session must quit, not leave an empty window** → `close_at` removes the window (Task 3); Task 4 checks it.
3. **Background sessions must keep running and stay current** → Task 4 check with a ticking clock.
4. **Keys must go to the active session only** → focus moves on every switch (Task 3); Task 4 checks it.
5. **Closing a session must not leave its shell running** → `Pty`'s `Drop` kills it (M1); Task 4 checks with `ps`.

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `src/session_list.rs` | Create | Ordered list + active index; switching/closing rules | 1 |
| `src/terminal_view.rs` | Replace | Emits `TerminalEvent`s; `Focusable`; no longer owns window title / quitting | 2 |
| `src/workspace.rs` | Create | Root view: sidebar, active pane, lifecycle, actions | 3 |
| `src/main.rs` | Replace | Key bindings; open the window with a `Workspace` | 3 |

---

### Task 1: `SessionList` — ⬜

**Files:** Create `src/session_list.rs` · Modify `src/main.rs` (add `mod session_list;`)

**Interfaces:** Produces `SessionList<T>` with `new()`, `items() -> &[T]`, `is_empty()`,
`active_index() -> Option<usize>`, `active() -> Option<&T>`, `get_mut(usize) -> Option<&mut T>`,
`position(impl Fn(&T) -> bool) -> Option<usize>`, `push(T) -> usize` (new item becomes active),
`select(usize) -> bool`, `select_next()`, `select_prev()` (both wrap), `remove(usize) -> Option<T>`.

- [ ] **Step 1: Write the failing tests.** Create `src/session_list.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn list(n: usize) -> SessionList<usize> {
        let mut l = SessionList::new();
        for i in 0..n {
            l.push(i);
        }
        l
    }

    #[test]
    fn push_makes_the_new_item_active() {
        let l = list(3);
        assert_eq!(l.active_index(), Some(2));
        assert_eq!(l.active(), Some(&2));
    }

    #[test]
    fn select_rejects_out_of_range() {
        let mut l = list(2);
        assert!(l.select(0));
        assert!(!l.select(5));
        assert_eq!(l.active_index(), Some(0));
    }

    #[test]
    fn next_and_prev_wrap() {
        let mut l = list(3);
        l.select_next();
        assert_eq!(l.active_index(), Some(0));
        l.select_prev();
        assert_eq!(l.active_index(), Some(2));
    }

    #[test]
    fn removing_the_active_item_activates_its_neighbour() {
        let mut l = list(3);
        l.select(1);
        assert_eq!(l.remove(1), Some(1));
        assert_eq!(l.active(), Some(&2)); // the next one slid into its place
        l.remove(1);
        assert_eq!(l.active(), Some(&0)); // removed the last: previous one
    }

    #[test]
    fn removing_before_the_active_item_keeps_it_active() {
        let mut l = list(3);
        l.select(2);
        l.remove(0);
        assert_eq!(l.active(), Some(&2));
        assert_eq!(l.active_index(), Some(1));
    }

    #[test]
    fn removing_the_last_item_leaves_nothing_active() {
        let mut l = list(1);
        l.remove(0);
        assert!(l.is_empty());
        assert_eq!(l.active_index(), None);
        assert_eq!(l.remove(0), None);
    }
}
```

…and add `mod session_list;` to `src/main.rs`.

- [ ] **Step 2: Run:** `cargo test session_list`. Expected: compile error (`SessionList` not found).
- [ ] **Step 3: Implement.** Put this above the test module:

```rust
//! The ordered list of sessions and which one is active. Generic and GPUI-free, so the
//! switching / closing rules are unit-tested.

pub struct SessionList<T> {
    items: Vec<T>,
    active: Option<usize>,
}

impl<T> SessionList<T> {
    pub fn new() -> Self {
        Self { items: Vec::new(), active: None }
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn active(&self) -> Option<&T> {
        self.active.map(|i| &self.items[i])
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.items.get_mut(index)
    }

    pub fn position(&self, pred: impl Fn(&T) -> bool) -> Option<usize> {
        self.items.iter().position(pred)
    }

    /// Append and make it active (a new session is where you want to type).
    pub fn push(&mut self, item: T) -> usize {
        self.items.push(item);
        let index = self.items.len() - 1;
        self.active = Some(index);
        index
    }

    /// Returns false (and changes nothing) when out of range.
    pub fn select(&mut self, index: usize) -> bool {
        if index < self.items.len() {
            self.active = Some(index);
            true
        } else {
            false
        }
    }

    /// Wraps around.
    pub fn select_next(&mut self) {
        if let Some(i) = self.active {
            self.active = Some((i + 1) % self.items.len());
        }
    }

    /// Wraps around.
    pub fn select_prev(&mut self) {
        if let Some(i) = self.active {
            self.active = Some((i + self.items.len() - 1) % self.items.len());
        }
    }

    /// Remove; if it was active, the next one (or the new last one) becomes active.
    pub fn remove(&mut self, index: usize) -> Option<T> {
        if index >= self.items.len() {
            return None;
        }
        let item = self.items.remove(index);
        self.active = match self.active {
            _ if self.items.is_empty() => None,
            Some(a) if a > index => Some(a - 1),
            Some(a) if a == index => Some(index.min(self.items.len() - 1)),
            other => other,
        };
        Some(item)
    }
}
```

- [ ] **Step 4: Run:** `cargo test session_list`. Expected: `test result: ok. 6 passed` (dead-code warnings until Task 3).
- [ ] **Step 5: Commit** *(when you say so)*: `git add src/session_list.rs src/main.rs && git commit -m "feat(m3): session list model with tests"`

### Task 2: Make `TerminalView` embeddable — ⬜

**Files:** Replace `src/terminal_view.rs`

**Interfaces:**
- Produces: `pub enum TerminalEvent { TitleChanged(Option<SharedString>), Exited }`; `impl EventEmitter<TerminalEvent> for TerminalView`; `impl Focusable for TerminalView`; `pub fn hsla(Rgb) -> Hsla` (now public, the workspace uses it).
- Removes: the `title` field, setting the window title in `render`, and `cx.quit()` on exit. Those move to the workspace.

> **What changed vs M2, and why.** A view inside a workspace must not quit the app or own the
> window title, so it *reports* instead (`cx.emit(...)`) and the owner decides. `Focusable` lets the
> workspace move keyboard focus between terminals. Everything else (painting, keys, status
> tracking) is unchanged.

- [ ] **Step 1: Replace `src/terminal_view.rs` with:**

```rust
//! The GPUI view for one terminal: paints a `GridSnapshot`, turns keystrokes into PTY bytes,
//! tracks the agent status, and reports what happened to whoever owns it (the workspace).

use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, WindowSize};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, Font, FontStyle,
    FontWeight, Hsla, KeyDownEvent, Pixels, Rgba, SharedString, Task, TextAlign, TextRun,
    UnderlineStyle, Window, canvas, div, fill, font, point, prelude::*, px, size,
};

use crate::colors;
use crate::grid::{self, GridSnapshot};
use crate::keys;
use crate::pty;
use crate::status::{Status, StatusTracker, TAIL_LINES};
use crate::terminal::{TermDims, TermEvent, TerminalCore};

const FONT_FAMILY: &str = "Menlo";
const FONT_SIZE: f32 = 14.0;
/// GPUI's default line height (1.618 x size) is too airy for a terminal.
const LINE_HEIGHT_FACTOR: f32 = 1.3;
/// How often to run housekeeping (flush stuck synchronized updates, re-evaluate status).
const TICK: Duration = Duration::from_millis(50);

/// What a terminal tells its owner.
pub enum TerminalEvent {
    /// The program set (Some) or reset (None) the title via OSC 0/2.
    TitleChanged(Option<SharedString>),
    /// The shell exited; the session should close.
    Exited,
}

pub struct TerminalView {
    core: TerminalCore,
    focus_handle: FocusHandle,
    tracker: StatusTracker,
    /// Dropping a GPUI `Task` cancels it, so these live as long as the view.
    _events: Task<()>,
    _ticker: Task<()>,
}

impl EventEmitter<TerminalEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
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
        Self { core, focus_handle, tracker: StatusTracker::new(), _events: events, _ticker: ticker }
    }

    fn on_term_event(&mut self, event: TermEvent, cx: &mut Context<Self>) {
        match event {
            TermEvent::Output { notifications } => {
                self.tracker.on_output(Instant::now(), notifications);
                cx.notify();
            }
            TermEvent::Term(Event::Wakeup) => cx.notify(),
            // A real BEL (not an OSC terminator). Must come before the `TermEvent::Term(_)` catch-all.
            TermEvent::Term(Event::Bell) => self.tracker.on_bell(Instant::now()),
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
            TermEvent::Term(Event::Title(title)) => cx.emit(TerminalEvent::TitleChanged(Some(title.into()))),
            TermEvent::Term(Event::ResetTitle) => cx.emit(TerminalEvent::TitleChanged(None)),
            TermEvent::Exited => cx.emit(TerminalEvent::Exited),
            TermEvent::Term(_) => {}
        }
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        if self.core.flush_stale_sync() {
            cx.notify();
        }
        let core = &self.core;
        if let Some(status) = self.tracker.tick(Instant::now(), || core.snapshot().tail_text(TAIL_LINES)) {
            self.status_changed(status, cx);
        }
    }

    fn status_changed(&self, status: Status, cx: &mut Context<Self>) {
        eprintln!("[fuxx] status -> {status:?}"); // debug log, used by the M2 manual check
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
        if m.platform {
            return; // cmd-shortcuts belong to the app (cmd-q, cmd-t, ...)
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
            if let Some(status) = self.tracker.on_input(Instant::now()) {
                self.status_changed(status, cx);
            }
            cx.stop_propagation();
        }
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

pub fn hsla(c: Rgb) -> Hsla {
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

- [ ] **Step 2: Build:** `cargo build`. Expected: it compiles. Unused-code warnings about `TerminalEvent`/`session_list` are fine until Task 3.
  *(Between Task 2 and Task 3 the app doesn't quit when the shell exits. Task 3 restores that.)*
- [ ] **Step 3: Tests:** `cargo test`. Expected: `test result: ok. 52 passed`.
- [ ] **Step 4: Commit** together with Task 3.

### Task 3: `Workspace` (sidebar + active pane) and the new `main.rs` — ⬜

**Files:** Create `src/workspace.rs` · Replace `src/main.rs`

**Interfaces:**
- Consumes: `TerminalView::spawn`, `TerminalEvent`, `hsla` (Task 2); `SessionList` (Task 1).
- Produces: actions `workspace::{NewSession, CloseSession, NextSession, PrevSession, SelectSession1…9}`; `Workspace::new(window, cx)`. M4 extends `Session` and `on_terminal_event`.

> **GPUI notes.** `cx.subscribe_in(&view, window, Self::on_terminal_event)` calls us with the
> window for every event the view emits. The returned `Subscription` is stored in the `Session`:
> dropping it unsubscribes. Actions bubble from the focused terminal up to the workspace's root
> `div`, where `.on_action(...)` handles them. Key bindings are matched *before* `on_key_down`, so
> bound cmd-keys never reach the shell. macOS reports cmd-shift-] as key `}`, hence `"cmd-}"`.

- [ ] **Step 1: Create `src/workspace.rs`:**

```rust
//! The root view: a sidebar listing every session and the active session's terminal.
//! Owns session lifecycle (new / close / exit) and switching.

use gpui::{
    ClickEvent, Context, Entity, Focusable, SharedString, Subscription, Window, actions, div,
    prelude::*, px, rgb,
};

use crate::colors;
use crate::session_list::SessionList;
use crate::terminal_view::{TerminalEvent, TerminalView, hsla};

actions!(
    workspace,
    [
        NewSession,
        CloseSession,
        NextSession,
        PrevSession,
        SelectSession1,
        SelectSession2,
        SelectSession3,
        SelectSession4,
        SelectSession5,
        SelectSession6,
        SelectSession7,
        SelectSession8,
        SelectSession9,
    ]
);

const SIDEBAR_WIDTH: f32 = 220.0;

pub struct Session {
    pub id: u64,
    pub view: Entity<TerminalView>,
    pub title: Option<SharedString>,
    _subscription: Subscription,
}

impl Session {
    pub fn label(&self) -> SharedString {
        self.title.clone().unwrap_or_else(|| format!("Shell {}", self.id).into())
    }
}

pub struct Workspace {
    sessions: SessionList<Session>,
    next_id: u64,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self { sessions: SessionList::new(), next_id: 1 };
        this.add_session(window, cx);
        this
    }

    fn add_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = match TerminalView::spawn(window, cx) {
            Ok(view) => view,
            Err(err) => {
                eprintln!("fuxx: could not start a shell: {err}");
                return;
            }
        };
        let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
        let id = self.next_id;
        self.next_id += 1;
        self.sessions.push(Session { id, view, title: None, _subscription: subscription });
        self.focus_active(window, cx);
        cx.notify();
    }

    fn on_terminal_event(
        &mut self,
        view: &Entity<TerminalView>,
        event: &TerminalEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.sessions.position(|s| s.view.entity_id() == view.entity_id()) else {
            return;
        };
        match event {
            TerminalEvent::TitleChanged(title) => {
                if let Some(session) = self.sessions.get_mut(index) {
                    session.title = title.clone();
                }
                cx.notify();
            }
            TerminalEvent::Exited => self.close_at(index, window, cx),
        }
    }

    fn close_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        // Dropping the Session drops its view -> TerminalCore -> Pty, which kills the child.
        self.sessions.remove(index);
        if self.sessions.is_empty() {
            window.remove_window(); // last session gone: close the window (and quit)
            return;
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.select(index) {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.sessions.active() {
            session.view.focus_handle(cx).focus(window, cx);
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.sessions.active_index();
        div()
            .flex()
            .flex_col()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .p_1()
            .gap_1()
            .bg(rgb(0x111111))
            .border_r_1()
            .border_color(rgb(0x2a2a2a))
            .children(self.sessions.items().iter().enumerate().map(|(i, session)| {
                let is_active = active == Some(i);
                div()
                    .id(("session", i))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(if is_active { 0xf0f0f0 } else { 0x9a9a9a }))
                    .when(is_active, |row| row.bg(rgb(0x2a2a2a)))
                    .hover(|style| style.bg(rgb(0x222222)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.select(i, window, cx)))
                    .child(format!("{}  {}", i + 1, session.label()))
            }))
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self
            .sessions
            .active()
            .map(|s| format!("fuxx — {}", s.label()))
            .unwrap_or_else(|| "fuxx".into());
        window.set_window_title(&title);

        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(hsla(colors::BACKGROUND))
            .key_context("Workspace")
            .on_action(cx.listener(|this, _: &NewSession, window, cx| this.add_session(window, cx)))
            .on_action(cx.listener(|this, _: &CloseSession, window, cx| {
                if let Some(i) = this.sessions.active_index() {
                    this.close_at(i, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextSession, window, cx| {
                this.sessions.select_next();
                this.focus_active(window, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PrevSession, window, cx| {
                this.sessions.select_prev();
                this.focus_active(window, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectSession1, w, cx| this.select(0, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession2, w, cx| this.select(1, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession3, w, cx| this.select(2, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession4, w, cx| this.select(3, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession5, w, cx| this.select(4, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession6, w, cx| this.select(5, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession7, w, cx| this.select(6, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession8, w, cx| this.select(7, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession9, w, cx| this.select(8, w, cx)))
            .child(self.render_sidebar(cx))
            // Only the active terminal is rendered; background sessions keep running (their
            // reader threads and tickers live in their views, which the list keeps alive).
            .child(div().flex_1().h_full().children(self.sessions.active().map(|s| s.view.clone())))
    }
}
```

- [ ] **Step 2: Replace `src/main.rs`:**

```rust
// fuxx — a native GPU terminal for running AI coding agents.
// Milestone 3: many sessions in one window, a sidebar, and quick-switching.

mod colors;
mod detector;
mod grid;
mod keys;
mod pty;
mod session_list;
mod status;
mod terminal;
mod terminal_view;
mod workspace;

use gpui::{
    App, Bounds, KeyBinding, QuitMode, TitlebarOptions, WindowBounds, WindowOptions, actions,
    prelude::*, px, size,
};
use gpui_platform::application;

use workspace::{
    CloseSession, NewSession, NextSession, PrevSession, SelectSession1, SelectSession2,
    SelectSession3, SelectSession4, SelectSession5, SelectSession6, SelectSession7,
    SelectSession8, SelectSession9, Workspace,
};

actions!(fuxx, [Quit]);

fn main() {
    // macOS apps normally keep running with no windows; a terminal should quit instead.
    application().with_quit_mode(QuitMode::LastWindowClosed).run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-t", NewSession, None),
            KeyBinding::new("cmd-w", CloseSession, None),
            // macOS reports cmd-shift-] as key "}" (shift folded into the character).
            KeyBinding::new("cmd-}", NextSession, None),
            KeyBinding::new("cmd-{", PrevSession, None),
            KeyBinding::new("cmd-1", SelectSession1, None),
            KeyBinding::new("cmd-2", SelectSession2, None),
            KeyBinding::new("cmd-3", SelectSession3, None),
            KeyBinding::new("cmd-4", SelectSession4, None),
            KeyBinding::new("cmd-5", SelectSession5, None),
            KeyBinding::new("cmd-6", SelectSession6, None),
            KeyBinding::new("cmd-7", SelectSession7, None),
            KeyBinding::new("cmd-8", SelectSession8, None),
            KeyBinding::new("cmd-9", SelectSession9, None),
        ]);
        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());

        let bounds = Bounds::centered(None, size(px(1100.0), px(640.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions { title: Some("fuxx".into()), ..Default::default() }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Workspace::new(window, cx)),
        )
        .expect("failed to open the window");
        cx.activate(true);
    });
}
```

- [ ] **Step 3: Build with zero warnings:** `cargo build 2>&1 | grep -E "^(warning|error)" | grep -v "future version"`. Expected: **no output**.
- [ ] **Step 4: Tests:** `cargo test`. Expected: `test result: ok. 52 passed`.
- [ ] **Step 5: Commit** *(when you say so)*: `git add src/terminal_view.rs src/workspace.rs src/main.rs && git commit -m "feat(m3): many sessions with a sidebar and quick-switching"`

### Task 4: Use it for real — 👀 ⬜

`cargo run`, then:

- [ ] The window shows a **sidebar** (dark, about 220 px) with **"1  Shell 1"** highlighted, and the terminal on the right.
- [ ] **cmd-t** twice → **"2  Shell 2"** and **"3  Shell 3"** appear; the newest is highlighted and has the cursor.
- [ ] In session 1 run `while true; do date; sleep 1; done`. Switch to 2 (**click** it), wait 5 s, switch back (**cmd-1**). The clock **kept going** (no gap).
- [ ] Typing goes **only** to the highlighted session (type in 2, check 1 and 3 are untouched).
- [ ] **cmd-}** / **cmd-{** cycle forward and back, wrapping around.
- [ ] A program that sets a title (e.g. `printf '\033]0;build\007'`) renames its row to **"build"**, and the window title follows the active session.
- [ ] `exit` in session 2 → its row disappears and focus moves to a neighbour.
- [ ] **cmd-w** closes the active session. Then `ps -ax | grep -c '[z]sh'` in another terminal shows the closed shell is **gone**.
- [ ] Closing the **last** session closes the window and **quits** the app.
- [ ] The M2 status log still prints for every session (run from another terminal to see stderr).

### Task 5: Update the docs — ⬜

- [ ] [[43 - Milestone 3 - Many Sessions]]: tick the checklist; **Status:: ✅ Done**; record D6 shortcuts.
- [ ] [[40 - Roadmap]]: M3 → ✅.
- [ ] [[00 - Plan Tracker (Start Here)]]: M3 → ✅ 5/5; **Where we are** → M4 Task 0.
- [ ] [[60 - Progress Log]]: new entry.
- [ ] Commit docs *(when you say so)*.

## Verification log

| Date | Check | Result | Evidence |
|---|---|---|---|
| 2026-10-12 | Plan code compiles at pinned rev | ✅ 0 warnings | AI scratch build |
| 2026-10-12 | Plan unit tests | ✅ 44 passed | AI scratch `cargo test` |
| 2026-10-26 | M2 plan change carried over: `Output { notifications }` + the BEL arm; expected count 44 → 52 (M2 now has 8 more tests) | ⚠️ edited, not re-compiled | Re-verify when M3 starts |
| | Task 4 checklist | | 👀 |

## Known limitations

| Limitation | Lands in |
|---|---|
| No rename / reorder sessions, no agent presets, no session restore | M7 Session ergonomics |
| Only one session visible at a time | M8 Split panes (deferred) |
| cmd-w doesn't ask "kill running process?" | Revisit if it bites (not in the plan) |
| New sessions always start in `$HOME` | M7 (per-dir presets) |

## Related

[[00 - Plan Tracker (Start Here)]] · [[M2 Plan - Agent State Detection]] · [[M4 Plan - Notification Rings]]
