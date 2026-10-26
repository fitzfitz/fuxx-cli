---
tags: [tech, reference, api]
verified: 2026-10-12
---

# 31 - API Fact Sheet (pinned rev)

The exact APIs the M0–M4 plans use, **read from source at the pinned versions** (not from memory).
GPUI is pre-1.0 and changes between commits, and most online tutorials are out of date. When
something here disagrees with a blog post, this note wins *for this rev*. If you bump a version,
re-verify and update this note.

| Crate | Version / rev | Source checked |
|---|---|---|
| `gpui`, `gpui_platform` | git `zed-industries/zed` @ **`1d2a4b3f7f194184dccadfa7a091d16a06482752`** (gpui 0.2.2) | crate source at that commit, plus a compiled probe |
| `alacritty_terminal` | **0.26.0** (re-exports `vte` 0.15) | crates.io source; Zed's fork at that rev has identical Term/grid/event APIs |
| `portable-pty` | **0.9.0** | crates.io source |

## GPUI at `1d2a4b3f`

### Cargo features (easy to miss)
- `gpui_platform` needs **`font-kit`**, or macOS renders **no text** (just a log line).
- `gpui_platform` **`runtime_shaders`** compiles Metal shaders at startup, so no full Xcode is needed to build (decision D1).

### Changed vs older tutorials
- **No `Application::new()`.** Use `gpui_platform::application()`, then `.with_quit_mode(..)` and `.run(|cx: &mut App| ..)`.
- **No** `ViewContext` / `WindowContext` / `ModelContext` / `Model<T>` / `View<T>`. It's `Entity<T>` + `Context<T>`, and callbacks take `&mut Window` and `&mut App` separately.
- `window.focus(&handle, cx)` and `handle.focus(window, cx)` both take `cx` now.
- `ShapedLine::paint(origin, line_height, TextAlign::Left, None, window, cx)` takes **6 args**.
- `TextSystem::font_id` is private. Use `resolve_font(&font) -> FontId`.
- `cx.spawn(async move |this, cx| { .. })` uses native async closures; `this: WeakEntity<T>`, `cx: &mut AsyncApp`.
- macOS default `QuitMode` = **don't quit** when the last window closes. Use `QuitMode::LastWindowClosed`.
- **No default key bindings or menus.** cmd-q does nothing until bound.

### Signatures the plans rely on
```rust
gpui_platform::application() -> gpui::Application
Application::with_quit_mode(self, QuitMode) -> Self;  Application::run(self, impl FnOnce(&mut App))
App::open_window<V: Render>(&mut self, WindowOptions, impl FnOnce(&mut Window, &mut App) -> Entity<V>) -> anyhow::Result<WindowHandle<V>>
App::bind_keys([KeyBinding::new("cmd-q", Quit, None /* or Some("Context") */)])
App::on_action(|_: &Quit, cx: &mut App| ..);  App::quit(&self);  App::activate(&self, bool)
actions!(namespace, [ActionA, ActionB]);   // unit-struct actions
Bounds::centered(None, size(px(w), px(h)), cx);  TitlebarOptions { title: Some("..".into()), .. }

trait Render { fn render(&mut self, &mut Window, &mut Context<Self>) -> impl IntoElement }
AppContext::new(|cx: &mut Context<T>| T) -> Entity<T>      // use gpui::prelude::*
Context::notify();  Context::emit(event);  Context::listener(Fn(&mut T, &E, &mut Window, &mut Context<T>))
Context::subscribe_in(&Entity<E>, &Window, FnMut(&mut T, &Entity<E>, &Evt, &mut Window, &mut Context<T>)) -> Subscription
Context::observe_window_activation(&mut Window, FnMut(&mut T, &mut Window, &mut Context<T>)) -> Subscription
impl EventEmitter<MyEvent> for MyView {}                   // marker trait
trait Focusable { fn focus_handle(&self, &App) -> FocusHandle }
App::focus_handle() -> FocusHandle;  FocusHandle::focus(&self, &mut Window, &mut App);  FocusHandle::is_focused(&self, &Window) -> bool
Window::is_window_active() -> bool;  Window::set_window_title(&mut self, &str);  Window::remove_window(&mut self)

Context::spawn(async move |this: WeakEntity<T>, cx: &mut AsyncApp| ..) -> Task<R>   // drop = cancel → store in a field
WeakEntity::update(&self, cx, |this, cx| ..) -> anyhow::Result<R>                   // Err once the entity is dropped
AsyncApp::background_executor().timer(Duration) -> Task<()>
AppContext::background_spawn(fut: impl Future + Send + 'static) -> Task<R>         // .detach() to fire-and-forget

canvas(prepaint: FnOnce(Bounds<Pixels>, &mut Window, &mut App) -> T,
       paint:    FnOnce(Bounds<Pixels>, T, &mut Window, &mut App)) -> Canvas<T>    // give it .size_full()!
Window::paint_quad(fill(bounds, color));  outline(bounds, color, BorderStyle::Solid)
Window::text_system().shape_line(text: SharedString, font_size: Pixels, runs: &[TextRun], force_width: Option<Pixels>) -> ShapedLine  // no '\n'!
TextRun { len /*utf8 bytes*/, font: Font, color: Hsla, background_color, underline: Option<UnderlineStyle>, strikethrough }
TextSystem::resolve_font(&Font) -> FontId;  em_advance(FontId, Pixels) -> Result<Pixels>
Rgba { r, g, b, a }.into() -> Hsla;  rgb(0xRRGGBB);  px(f32);  point(x, y);  size(w, h);  Bounds::new(origin, size)
```

### Keyboard events on macOS
`KeyDownEvent { keystroke: Keystroke { key, key_char, modifiers }, .. }`; `Modifiers { control, alt, shift, platform /*cmd*/, function }`.

| Press | `key` | `key_char` |
|---|---|---|
| a / shift-a | `"a"` | `"a"` / `"A"` (shift = true) |
| shift-1 | `"!"` | `"!"` (**shift reported false**) |
| ctrl-c | `"c"` | **None** (ctrl or cmd held → None) |
| enter / tab / space | `"enter"` / `"tab"` / `"space"` | `"\n"` / `"\t"` / `" "` |
| arrows, backspace, escape, home, end, pageup, pagedown, delete, f1–f35 | the name | None |
| option-s | `"s"` | `"ß"` (composed) |

Dispatch order: the **keymap matches first**. If a binding matches, `on_key_down` is never called.
cmd-shift-] arrives as key `"}"`, so bind `"cmd-}"`.

## alacritty_terminal 0.26.0

```rust
Term::new<D: Dimensions>(Config::default(), &dims, listener) -> Term<L>
trait Dimensions { fn total_lines(&self) -> usize; fn screen_lines(&self) -> usize; fn columns(&self) -> usize; }
term.resize(dims /* by value */);   *term.mode() -> TermMode (APP_CURSOR, BRACKETED_PASTE, ...);   term.colors() -> &Colors
term.renderable_content() -> RenderableContent { display_iter, cursor: RenderableCursor { shape, point }, display_offset, colors, mode, .. }
  // viewport row = point.line.0 + display_offset   (grid lines go negative when scrolled back)
Cell { c: char, fg: Color, bg: Color, flags: Flags /* INVERSE BOLD ITALIC DIM HIDDEN WIDE_CHAR(_SPACER) ALL_UNDERLINES ... */ }
vte::ansi::Color { Named(NamedColor), Spec(Rgb), Indexed(u8) };  Colors: 269 Option<Rgb> slots, ALL None unless a program set them
vte::ansi::Processor::<StdSyncHandler>::new();  processor.advance(&mut term, &bytes /* slice */)
processor.sync_timeout().sync_timeout() -> Option<Instant>;  processor.stop_sync(&mut term)   // flush a stuck sync update
alacritty_terminal::sync::FairMutex<T>   // public; lock() / lock_unfair()
trait EventListener { fn send_event(&self, Event) }   // called INSIDE advance, with the Term lock held. Never lock Term here
Event::{ PtyWrite(String) /* MUST be written back to the PTY */, ColorRequest(idx, fmt), TextAreaSizeRequest(fmt), Title(String), ResetTitle, Bell, Wakeup, ... }
```
**M5 additions (scrollback, selection, links):**
```rust
alacritty_terminal::grid::Scroll { Delta(i32) /* + = up into history */, PageUp, PageDown, Top, Bottom };  term.scroll_display(scroll)
term.grid().display_offset();  term.grid()[Line(r)][Column(c)]  // Line(0..screen_lines) is ALWAYS the live screen
alacritty_terminal::index::{Point, Line(i32), Column(usize), Side /* = Direction::{Left, Right} */, Boundary}
point.grid_clamp(&term, Boundary::Grid)   // clamp a possibly stale point before indexing (indexing out of range panics)
Selection::new(SelectionType::{Simple, Semantic, Lines}, point, side);  sel.update(point, side);  term.selection = Some(sel)
term.selection_to_string() -> Option<String>;  renderable_content().selection: Option<SelectionRange> /* .contains(grid point) */
// Term::resize clears the selection when the column count changes
term::search::{RegexSearch::new(&str), RegexIter::new(start, end, Direction::Right, &term, &mut regex) /* Iterator<Item = Match = RangeInclusive<Point>> */}
term.line_search_left(p) / line_search_right(p);  term.bounds_to_string(start, end);  cell.hyperlink().map(|h| h.uri())  // OSC 8
Config::default(): scrolling_history 10_000, kitty_keyboard false (so `ESC[?u` gets no reply)
```

`Wakeup` and `ChildExit` are only sent by alacritty's own event loop, which we don't use. Our reader thread signals redraw/exit itself.

## GPUI mouse, clipboard, links (M5)

```rust
MouseDownEvent { button, position, modifiers, click_count, first_mouse };  MouseUpEvent { button, position, modifiers, click_count }
MouseMoveEvent { position, pressed_button: Option<MouseButton>, modifiers };  ScrollWheelEvent { position, delta, modifiers, touch_phase }
delta.pixel_delta(line_height) -> Point<Pixels>  // + y = scroll up;  TouchPhase::Started resets an accumulator
div().on_mouse_down(MouseButton::Left, ..) / on_scroll_wheel / on_modifiers_changed / on_action   // div listeners fire only while hovered
window.on_mouse_event::<MouseMoveEvent>(|e, phase, window, cx| ..)   // paint phase only; sees drags that leave the element
window.insert_hitbox(bounds, HitboxBehavior::Normal) /* prepaint */;  window.set_cursor_style(CursorStyle::PointingHand, &hitbox) /* paint */
cx.write_to_clipboard(ClipboardItem::new_string(s));  cx.read_from_clipboard()?.text();  cx.open_url(&url)
f32::from(pixels)   // Pixels → f32
```
Key **bindings run before** a view's `on_key_down` (window.rs ~4905–4960); `cx.propagate()` in an action lets the key fall through.

## portable-pty 0.9.0

```rust
native_pty_system().openpty(PtySize { rows, cols, pixel_width, pixel_height }) -> anyhow::Result<PtyPair>   // M5: real pixels = cells × cell size
CommandBuilder::new_default_prog()   // login shell ($SHELL, argv0 "-zsh"); PANICS if you call .arg() on it
cmd.cwd(dir); cmd.env("TERM", "xterm-256color");   // &mut self, returns (): not chainable
pair.slave.spawn_command(cmd)?;  drop(pair.slave);  // drop the slave or the reader never sees EOF
pair.master.try_clone_reader()?;  pair.master.take_writer()?  // writer ONCE; dropping it sends EOF (^D) to the child
pair.master.resize(PtySize { .. })?;  child.kill()
```
On macOS, `read` hits `EIO` at child exit, and portable-pty maps that to `Ok(0)` (plain EOF).

## Key → bytes table (xterm)

Implemented and unit-tested in `src/keys.rs` ([[M1 Plan - One Terminal Pane]] Task 1). Based on Zed's
`crates/terminal/src/mappings/keys.rs`, with three Zed bugs fixed: modified F5 never matched (`"F5"`
vs `"f5"`), there was no modified Delete, and alt+named-key sent `"\x1bspace"`.

| Key | Bytes | Notes |
|---|---|---|
| enter / backspace / tab / esc | `\r` / `\x7f` / `\t` / `\x1b` | shift-tab `\x1b[Z`; ctrl-backspace `\x08` |
| arrows | `\x1b[A..D` | **app-cursor mode: `\x1bOA..D`** |
| ctrl + a..z | `\x01`..`\x1a` | ctrl-[ `\x1b`, ctrl-space `\x00` |
| modified arrows/home/end/F1–4 | `\x1b[1;{m}X` | m = 1 + shift + 2·alt + 4·ctrl |
| F5–F12, ins/del/pgup/pgdn | `\x1b[N~` / `\x1b[N;{m}~` | |

## Related

[[30 - Tech Stack]] · [[00 - Plan Tracker (Start Here)]] · [[52 - Known Risks and Gotchas]]
