//! The file tab's Markdown Preview (M11ac, M11ae) and its Mermaid diagrams (real WebKit, in a
//! child run).

use super::*;

/// This area's scenes (`mod.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "file-tab-md",
        run: file_tab_md_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/md_preview.rs",
            "src/agent_markdown.rs",
            "src/scrollbar.rs",
            "src/name_input.rs",
        ],
    },
    Scene {
        name: "file-tab-md-mermaid",
        run: file_tab_md_mermaid_scenes,
        themes: Themes::One,
        serial: true,
        kind: Kind::State,
        sources: &[
            "src/file_tab.rs",
            "src/md_preview.rs",
            "src/mermaid.rs",
            "src/mermaid_render.rs",
            "src/agent_markdown.rs",
            "src/browser.rs",
        ],
    },
];

/// The README the Preview scenes show (M11ac): a little of everything the reader draws.
const MD_FIXTURE: &str = r#"# fuxx preview

A **native** terminal for *AI agents*, with `inline code` and a [link](https://example.com).

## Tasks

- [x] Parse the Markdown
- [ ] Draw it as a reader
  - a nested bullet
    1. deeper, numbered

> A block quote, with **bold** inside.

| Feature | State | Notes |
|:--|:-:|--:|
| Tables | done | right |
| Task lists | done | ☑ |

---

```rust
fn main() {
    println!("hi");
}
```

![logo](logo.png) Images show their alt text.
"#;

/// M11ac's Markdown Preview in a file tab, on `MD_FIXTURE` (README.md), 760 × 600, in both
/// themes:
/// - `file-tab-md-source-with-toggle`: the source as before, the view switch File | Preview
///   with File current (M11ae). A `.rs` file has no Preview segment.
/// - `file-tab-md-preview`: after a click on Preview: the segment current, the file drawn as a
///   reader (headings by level, the paragraph's bold, italic, inline code and link, ☑ / ☐ tasks, a
///   nested bullet and number, the quote's bar, the table's grid, the rule, the code block on
///   its fill, the image's alt text in brackets). The tab told its workspace (`Preview(true)`).
///
/// - `file-tab-md-preview-scrollbar` (M11ae): a 300-paragraph file in Preview: a wheel step shows
///   the thumb in the list's right 12 pt; the thumb dragged below the list scrolls to the end
///   (shot: the thumb at the bottom right). A short file never shows one.
///
/// Not shot: a long file scrolled down in the Preview, rewritten on disk and read again: parsed
/// again, the scroll kept; ⌘F then turns Preview off (`Preview(false)`) and opens the find
/// bar on the source; Diff mode shows no Preview.
fn file_tab_md_scenes() {
    use crate::file_tab::{FileTab, FileTabEvent};
    use crate::layout_model::FileMode;
    use crate::md_preview::Colours;
    use gpui::{Focusable, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-md");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let readme = dir.join("README.md");
    std::fs::write(&readme, MD_FIXTURE).expect("README.md");
    let code = dir.join("main.rs");
    std::fs::write(&code, "fn main() {}\n").expect("main.rs");
    let long = dir.join("LONG.md");
    let long_text = |title: &str| {
        let mut t = format!("# {title}\n\n");
        for i in 0..60 {
            t.push_str(&format!("Paragraph {i}, long enough to read as prose.\n\n"));
        }
        t
    };
    std::fs::write(&long, long_text("Long")).expect("LONG.md");
    // Review fixes: lines of 16,000 `>` (quotes nest 16 deep at most; a line under the 20,000
    // a coloured line may have, so the file is Preview's), and a file past Preview's 1 MB.
    let deep = dir.join("DEEP.md");
    std::fs::write(&deep, format!("{}\n", ">".repeat(16_000)).repeat(4)).expect("DEEP.md");
    let scrolly = dir.join("SCROLLY.md");
    std::fs::write(
        &scrolly,
        (0..300)
            .map(|i| format!("Paragraph {i}, long enough to read as prose.\n\n"))
            .collect::<String>(),
    )
    .expect("SCROLLY.md");
    // Over `PREVIEW_MEASURE_ALL` blocks: its rows are not all measured, the thumb is estimated.
    let many = dir.join("MANY.md");
    std::fs::write(
        &many,
        (0..3_000)
            .map(|i| format!("Paragraph {i}, long enough to read as prose.\n\n"))
            .collect::<String>(),
    )
    .expect("MANY.md");
    let tiny = dir.join("TINY.md");
    std::fs::write(&tiny, "# Tiny\n\nOne short paragraph.\n").expect("TINY.md");
    let huge = dir.join("HUGE.md");
    std::fs::write(&huge, "word ".repeat(crate::file_tab::PREVIEW_MAX / 5 + 1)).expect("HUGE.md");

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    type Events = Rc<RefCell<Vec<bool>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    let dims = size(px(760.0), px(600.0));
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let build_for = |path: PathBuf, slot: TabSlot, events: Events| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(path.clone(), FileMode::Plain, window, cx);
                    tab.frozen = true;
                    tab
                });
                let seen = events.clone();
                cx.subscribe(&tab, move |_, e: &FileTabEvent, _| {
                    if let FileTabEvent::Preview(on) = e {
                        seen.borrow_mut().push(*on);
                    }
                })
                .detach();
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };

        // The source, with Preview (off) in the view switch; none for a .rs file.
        let n = format!("file-tab-md-source-with-toggle-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(readme.clone(), slot.clone(), Rc::default());
        let act = {
            let n = n.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                frame(cx, window);
                let tab = tab_of(&slot);
                cx.read(|cx| {
                    let t = tab.read(cx);
                    assert_eq!(t.state_name(), "text", "{n}");
                    assert!(!t.preview_shown(), "{n}: off by default");
                    assert!(t.preview_bounds().is_some(), "{n}: Preview is drawn");
                });
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-source-with-toggle",
            slug,
            &theme,
            dims,
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
        let slot: TabSlot = Rc::default();
        let build = build_for(code.clone(), slot.clone(), Rc::default());
        render_with(&theme, dims, build, move |cx, window| {
            frame(cx, window);
            let tab = tab_of(&slot);
            let drawn = cx.read(|cx| tab.read(cx).preview_bounds());
            assert!(drawn.is_none(), "a .rs file has no Preview segment");
            slot.borrow_mut().take();
        });

        // A click on Preview: the reader.
        let n = format!("file-tab-md-preview-{slug}");
        let slot: TabSlot = Rc::default();
        let events: Events = Rc::default();
        let build = build_for(readme.clone(), slot.clone(), events.clone());
        let act = {
            let n = n.clone();
            let events = events.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                // Shot, then rendered again for the checks: each run counts its own.
                events.borrow_mut().clear();
                frame(cx, window);
                let tab = tab_of(&slot);
                let button = cx
                    .read(|cx| tab.read(cx).preview_bounds())
                    .expect("Preview is drawn");
                cx.simulate_click(window, button.center(), Modifiers::none());
                frame(cx, window);
                cx.read(|cx| {
                    let t = tab.read(cx);
                    assert!(t.preview_shown(), "{n}: a click turns it on");
                    assert!(t.parsed_version().is_some(), "{n}: parsed");
                });
                assert_eq!(*events.borrow(), [true], "{n}: the workspace is told");
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-preview",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        // The code block's fill and the link's colour are drawn somewhere in the body.
        let colours = Colours::of(&theme);
        let near = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= by)
        };
        let found = |c, by| {
            (40..600).step_by(2).any(|y| {
                (0..760)
                    .step_by(2)
                    .any(|x| near(pixel_at(&shot, dims, x as f32, y as f32), c, by))
            })
        };
        assert!(found(colours.code_bg, 2), "{n}: the code's fill");
        assert!(found(colours.link, 40), "{n}: the link's colour");

        // M11ae Task 4: the scrollbar in Preview. A long file: none before a scroll, shown by a
        // wheel step, a thumb drag to below the list scrolls it to the end; a short file never
        // shows one.
        {
            use crate::scrollbar::Axis;
            use gpui::{MouseButton, ScrollDelta, ScrollWheelEvent, TouchPhase};
            let n = format!("file-tab-md-preview-scrollbar-{slug}");
            let slot: TabSlot = Rc::default();
            let build = build_for(scrolly.clone(), slot.clone(), Rc::default());
            let tiny = tiny.clone();
            let short_slot: TabSlot = Rc::default();
            let short_build = build_for(tiny, short_slot.clone(), Rc::default());
            let none = Modifiers::none();
            let wheel = |cx: &mut VisualTestAppContext, window, at| {
                cx.simulate_event(
                    window,
                    ScrollWheelEvent {
                        position: at,
                        delta: ScrollDelta::Lines(point(0.0, -3.0)),
                        modifiers: none,
                        touch_phase: TouchPhase::Moved,
                    },
                );
            };
            let preview = |cx: &mut VisualTestAppContext, window, tab: &gpui::Entity<FileTab>| {
                frame(cx, window);
                let button = cx
                    .read(|cx| tab.read(cx).preview_bounds())
                    .expect("Preview is drawn");
                cx.simulate_click(window, button.center(), none);
                frame(cx, window);
            };
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let tab = tab_of(&slot);
                    preview(cx, window, &tab);
                    let list = cx.read(|cx| tab.read(cx).preview_list());
                    let area = list.viewport_bounds();
                    assert!(
                        !cx.read(|cx| tab.read(cx).scrollbar_shown()),
                        "{n}: shown before any scroll"
                    );
                    wheel(cx, window, area.center());
                    frame(cx, window);
                    assert!(
                        cx.read(|cx| tab.read(cx).scrollbar_shown()),
                        "{n}: not shown after a wheel step"
                    );
                    let thumb = cx
                        .read(|cx| tab.read(cx).thumb_bounds(Axis::Vertical))
                        .expect("a thumb");
                    assert!(
                        thumb.left() >= area.right() - px(12.0) && thumb.right() <= area.right(),
                        "{n}: the thumb {thumb:?} is not in the list's right 12 pt ({area:?})"
                    );
                    // Drag the thumb to below the list: the end.
                    let grab = thumb.center();
                    cx.simulate_mouse_down(window, grab, MouseButton::Left, none);
                    let below = point(grab.x, area.bottom() + px(40.0));
                    cx.simulate_mouse_move(window, below, Some(MouseButton::Left), none);
                    cx.simulate_mouse_up(window, below, MouseButton::Left, none);
                    frame(cx, window);
                    assert_eq!(
                        list.is_scrolled_to_end(),
                        Some(true),
                        "{n}: dragged to the end"
                    );
                    assert!(!list.is_scrollbar_dragging(), "{n}: the drag is over");
                    let thumb = cx
                        .read(|cx| tab.read(cx).thumb_bounds(Axis::Vertical))
                        .expect("a thumb");
                    assert!(
                        (thumb.bottom() - area.bottom()).abs() < px(1.0),
                        "{n}: the thumb {thumb:?} at the track's end"
                    );
                    assert!(
                        cx.read(|cx| tab.read(cx).scrollbar_shown()),
                        "{n}: not shown after a drag"
                    );
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(
                "file-tab-md-preview-scrollbar",
                slug,
                &theme,
                dims,
                build,
                act,
            );
            println!("ui-shots: {}", path.display());
            let n = format!("file-tab-md-preview-scrollbar-short-{slug}");
            render_with(&theme, dims, short_build, move |cx, window| {
                let tab = tab_of(&short_slot);
                preview(cx, window, &tab);
                let area = cx.read(|cx| tab.read(cx).preview_list().viewport_bounds());
                wheel(cx, window, area.center());
                cx.simulate_mouse_move(
                    window,
                    point(area.right() - px(4.0), area.center().y),
                    None,
                    none,
                );
                frame(cx, window);
                assert!(
                    !cx.read(|cx| tab.read(cx).scrollbar_shown()),
                    "{n}: shown for a file that fits"
                );
                short_slot.borrow_mut().take();
            });

            // Past the measure-all size: the list's rows are measured as drawn, and a thumb
            // dragged to the bottom still lands on the true end.
            let n = format!("file-tab-md-preview-scrollbar-many-{slug}");
            let many_slot: TabSlot = Rc::default();
            let many_build = build_for(many.clone(), many_slot.clone(), Rc::default());
            render_with(&theme, dims, many_build, move |cx, window| {
                let tab = tab_of(&many_slot);
                preview(cx, window, &tab);
                let list = cx.read(|cx| tab.read(cx).preview_list());
                let area = list.viewport_bounds();
                assert!(
                    list.item_count() > crate::file_tab::PREVIEW_MEASURE_ALL,
                    "{n}"
                );
                wheel(cx, window, area.center());
                frame(cx, window);
                let thumb = cx
                    .read(|cx| tab.read(cx).thumb_bounds(Axis::Vertical))
                    .expect("a thumb");
                let grab = thumb.center();
                cx.simulate_mouse_down(window, grab, MouseButton::Left, none);
                let below = point(grab.x, area.bottom() + px(40.0));
                cx.simulate_mouse_move(window, below, Some(MouseButton::Left), none);
                cx.simulate_mouse_up(window, below, MouseButton::Left, none);
                frame(cx, window);
                // (`is_scrolled_to_end` is None with unmeasured rows: the last block's bottom is
                // the list's bottom instead.)
                let last = list
                    .bounds_for_item(list.item_count() - 1)
                    .expect("the last block is drawn");
                assert!(
                    (last.bottom() - area.bottom()).abs() <= px(1.0),
                    "{n}: dragged to the end, the last block {last:?} in {area:?}"
                );
                many_slot.borrow_mut().take();
            });
        }

        // A long file, scrolled, rewritten: parsed again, the scroll kept; then ⌘F turns
        // Preview off and opens find; Diff mode shows none (not shot).
        let slot: TabSlot = Rc::default();
        let events: Events = Rc::default();
        let build = build_for(long.clone(), slot.clone(), events.clone());
        let long = long.clone();
        render_with(&theme, dims, build, move |cx, window| {
            let n = format!("file-tab-md-reload-{slug}");
            frame(cx, window);
            let tab = tab_of(&slot);
            let button = cx.read(|cx| tab.read(cx).preview_bounds()).expect("drawn");
            cx.simulate_click(window, button.center(), Modifiers::none());
            frame(cx, window);
            let rows = cx.read(|cx| tab.read(cx).preview_list());
            rows.scroll_to(gpui::ListOffset {
                item_ix: 20,
                offset_in_item: px(5.0),
            });
            frame(cx, window);
            let before = cx.read(|cx| tab.read(cx).parsed_version());
            std::fs::write(&long, long_text("Long, edited")).expect("rewrite");
            cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
            cx.run_until_parked();
            frame(cx, window);
            let after = cx.read(|cx| tab.read(cx).parsed_version());
            assert!(after > before, "{n}: parsed again ({before:?} → {after:?})");
            let top = rows.logical_scroll_top();
            assert_eq!(
                (top.item_ix, top.offset_in_item),
                (20, px(5.0)),
                "{n}: the scroll stays"
            );
            cx.update_window(window, |_, window, cx| {
                tab.read(cx).focus_handle(cx).focus(window, cx)
            })
            .expect("focus");
            cx.simulate_keystrokes(window, "cmd-f");
            frame(cx, window);
            cx.read(|cx| {
                let t = tab.read(cx);
                assert!(!t.preview_shown(), "{n}: ⌘F shows the source");
                assert!(t.find_open(), "{n}: with the find bar");
            });
            assert_eq!(*events.borrow(), [true, false], "{n}: both saved");
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_mode(FileMode::Diff, None, cx);
                })
            });
            frame(cx, window);
            assert!(
                !cx.read(|cx| tab.read(cx).preview_shown()),
                "{n}: none in Diff"
            );
            slot.borrow_mut().take();
        });

        // 64,000 `>` in Preview: drawn (16 quotes deep, the rest text), no stack overflow.
        let slot: TabSlot = Rc::default();
        let build = build_for(deep.clone(), slot.clone(), Rc::default());
        render_with(&theme, dims, build, move |cx, window| {
            frame(cx, window);
            let tab = tab_of(&slot);
            let button = cx.read(|cx| tab.read(cx).preview_bounds()).expect("drawn");
            cx.simulate_click(window, button.center(), Modifiers::none());
            frame(cx, window);
            assert!(
                cx.read(|cx| tab.read(cx).preview_shown()),
                "deep quotes: shown"
            );
            slot.borrow_mut().take();
        });

        // Over 1 MB: the segment is drawn (faint) but does nothing; the source stays.
        let slot: TabSlot = Rc::default();
        let events: Events = Rc::default();
        let build = build_for(huge.clone(), slot.clone(), events.clone());
        render_with(&theme, dims, build, move |cx, window| {
            frame(cx, window);
            let tab = tab_of(&slot);
            let button = cx.read(|cx| tab.read(cx).preview_bounds());
            let button = button.expect("drawn, faint");
            cx.simulate_click(window, button.center(), Modifiers::none());
            frame(cx, window);
            assert!(
                !cx.read(|cx| tab.read(cx).preview_shown()),
                "over 1 MB: source"
            );
            assert!(events.borrow().is_empty(), "over 1 MB: nothing saved");
            slot.borrow_mut().take();
        });
    }
}

/// M11ac Part 2's fixture: a flowchart, a sequence diagram and a broken diagram.
const MERMAID_FIXTURE: &str = r#"# Diagrams

A flowchart:

```mermaid
flowchart LR
  A[Agent] -->|edits| B(File tab)
  B --> C{Preview?}
  C -->|on| D[Diagram]
  C -->|off| E[Source]
```

A sequence diagram:

```Mermaid
sequenceDiagram
  participant You
  participant fuxx
  You->>fuxx: open README.md
  fuxx-->>You: Preview, with diagrams
```

A broken one keeps its code:

```mermaid
flowchart TD
  A -->
```
"#;

/// M11ac Part 2: Mermaid diagrams in the Markdown Preview, on `MERMAID_FIXTURE`, 760 × 980,
/// in Mocha (both themes with `--all-themes`):
/// - `file-tab-md-mermaid-pending`: the first frame after the click on Preview, before WebKit
///   answers: every block still code, no notes.
/// - `file-tab-md-mermaid`: the hidden web view (real WebKit) renders the flowchart and the
///   sequence diagram, shown as images at their natural size, centred; the broken one stays
///   code with "Mermaid: <error>" under it in the dim colour. Checked: 2 diagrams, 1 note
///   starting with "Mermaid: ".
///
/// WebKit answers through the main run loop, which the off-screen harness never turns, so this
/// scene turns it by hand. Doing that in a process where earlier scenes dropped their windows
/// runs GPUI's deferred window closes for apps already gone (they panic), so the rendered half
/// runs in a child process (`FUXX_SHOTS_WEBKIT`), first and alone, its contexts kept alive.
fn file_tab_md_mermaid_scenes() {
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use crate::mermaid_render::Mermaids;
    use gpui::Modifiers;
    use objc2_foundation::{NSDate, NSRunLoop};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    const CHILD: &str = "FUXX_SHOTS_WEBKIT";
    let child = std::env::var_os(CHILD).is_some();
    let dir = std::env::temp_dir().join("fuxx-ui-shots-mermaid");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let readme = dir.join("README.md");
    std::fs::write(&readme, MERMAID_FIXTURE).expect("README.md");
    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let dims = size(px(760.0), px(980.0));
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    let build_for = |theme: Theme, slot: TabSlot| {
        let path = readme.clone();
        move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let tab = cx.new(|cx| {
                let mut tab = FileTab::new(path.clone(), FileMode::Plain, window, cx);
                tab.frozen = true;
                tab
            });
            *slot.borrow_mut() = Some(tab.clone());
            tab.into()
        }
    };
    let click_preview =
        move |cx: &mut VisualTestAppContext, window, tab: &gpui::Entity<FileTab>| {
            frame(cx, window);
            let button = cx
                .read(|cx| tab.read(cx).preview_bounds())
                .expect("Preview is drawn");
            cx.simulate_click(window, button.center(), Modifiers::none());
            frame(cx, window);
        };

    if !child {
        // Before WebKit answers: code, no notes.
        for &(slug, name) in shot_themes() {
            let theme = shot_theme(name);
            let n = format!("file-tab-md-mermaid-pending-{slug}");
            let slot: TabSlot = Rc::default();
            let act = {
                let slot = slot.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let tab = slot.borrow().clone().expect("the tab");
                    click_preview(cx, window, &tab);
                    let (diagrams, notes) = cx.read(|cx| tab.read(cx).mermaid_drawn());
                    assert_eq!((diagrams, notes.len()), (0, 0), "{n}: still code");
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(
                "file-tab-md-mermaid-pending",
                slug,
                &theme,
                dims,
                build_for(theme, slot.clone()),
                act,
            );
            println!("ui-shots: {}", path.display());
        }
        let spawned = Instant::now();
        let status = child_run("file-tab-md-mermaid")
            .env(CHILD, "1")
            .status()
            .expect("run the WebKit half");
        prof::add(prof::CHILD, spawned.elapsed());
        assert!(status.success(), "file-tab-md-mermaid (WebKit): {status}");
        return;
    }

    // The child: rendered, the run loop turned by hand until both diagrams and the error are in.
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let n = format!("file-tab-md-mermaid-{slug}");
        let slot: TabSlot = Rc::default();
        let mut cx = gpui::VisualTestAppContext::new(current_platform(false));
        fonts::register_bundled_fonts(&cx.text_system().all_font_names());
        let build = build_for(theme, slot.clone());
        let window = cx
            .open_offscreen_window(dims, move |window, cx| {
                let scene = build(window, cx);
                cx.new(|_| Root { theme, scene })
            })
            .expect("open an off-screen window");
        cx.run_until_parked();
        let window: AnyWindowHandle = window.into();
        let tab = slot.borrow().clone().expect("the tab");
        click_preview(&mut cx, window, &tab);
        let started = Instant::now();
        let mut drawn = (0, Vec::new());
        while started.elapsed() < Duration::from_secs(30) {
            NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.05));
            frame(&mut cx, window);
            drawn = cx.read(|cx| tab.read(cx).mermaid_drawn());
            if drawn.0 + drawn.1.len() == 3 {
                break;
            }
        }
        println!(
            "ui-shots: {n}: {} diagrams, notes {:?}, after {:?}",
            drawn.0,
            drawn.1,
            started.elapsed()
        );
        assert_eq!(drawn.0, 2, "{n}: the flowchart and the sequence diagram");
        assert_eq!(drawn.1.len(), 1, "{n}: the broken one's note");
        assert!(drawn.1[0].starts_with("Mermaid: "), "{n}: {:?}", drawn.1);
        // A few more frames, so GPUI's decoding of the images (off the UI thread) is in.
        for _ in 0..10 {
            NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.05));
            cx.advance_clock(Duration::from_millis(250));
            frame(&mut cx, window);
        }
        let image = cx.capture_screenshot(window).expect("capture the window");
        let shot = Shot {
            width: image.width(),
            height: image.height(),
            rgba: image.as_raw().to_vec(),
        };
        let path = keep_shot(&format!("file-tab-md-mermaid-{slug}"), &shot);
        println!("ui-shots: {}", path.display());
        let turn = |cx: &mut VisualTestAppContext| {
            NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.05));
            frame(cx, window);
        };

        // Scrolling reads the cache: nothing renders again.
        let before = cx.update(Mermaids::counts);
        let (cached, bytes, queued, running) = before;
        assert_eq!(
            (cached, queued, running),
            (3, 0, false),
            "{n}: 2 diagrams and 1 error cached, none waiting"
        );
        assert!(
            bytes > 0 && bytes <= crate::mermaid::CACHE_BYTES,
            "{n}: {bytes} bytes"
        );
        println!("ui-shots: {n}: {bytes} bytes of decoded images cached");
        let rows = cx.read(|cx| tab.read(cx).preview_list());
        for item_ix in [3, 0, 5, 1] {
            rows.scroll_to(gpui::ListOffset {
                item_ix,
                offset_in_item: px(0.0),
            });
            turn(&mut cx);
        }
        assert_eq!(
            cx.update(Mermaids::counts),
            before,
            "{n}: scrolling rendered again"
        );

        // The other theme (light ↔ dark): every diagram rendered again for it.
        let other = shot_theme(if slug == "mocha" {
            "Catppuccin Latte"
        } else {
            "Catppuccin Mocha"
        });
        cx.update(|cx| cx.set_global(shot_appearance(other, cx)));
        frame(&mut cx, window);
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(30) {
            turn(&mut cx);
            let (cached, _, queued, running) = cx.update(Mermaids::counts);
            if (cached, queued, running) == (6, 0, false) {
                break;
            }
        }
        let (cached, _, queued, running) = cx.update(Mermaids::counts);
        assert_eq!(
            (cached, queued, running),
            (6, 0, false),
            "{n}: rendered for the other theme"
        );
        let drawn = cx.read(|cx| tab.read(cx).mermaid_drawn());
        assert_eq!(
            (drawn.0, drawn.1.len()),
            (2, 1),
            "{n}: after the theme change"
        );
        println!("ui-shots: {n}: the theme change rendered again, scrolling didn't");
        // Kept alive: dropping it queues window closes that would run (and panic) the next
        // time the run loop turns.
        std::mem::forget(cx);
    }

    // No network: a diagram whose label holds an <img>, whose init loads a stylesheet, and
    // which has a click link, all to a server on this machine: nothing connects to it.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a local port");
    listener.set_nonblocking(true).expect("non-blocking");
    let port = listener.local_addr().expect("its address").port();
    let net = dir.join("NET.md");
    std::fs::write(
        &net,
        format!(
            "# Net\n\n```mermaid\n%%{{init: {{\"themeCSS\": \"@import url(http://127.0.0.1:{port}/a.css);\"}}}}%%\n\
             flowchart LR\n  A[\"<img src='http://127.0.0.1:{port}/a.png'> pic\"] --> B\n  \
             click A href \"http://127.0.0.1:{port}/click\"\n```\n"
        ),
    )
    .expect("NET.md");
    let theme = shot_theme("Catppuccin Mocha");
    let slot: TabSlot = Rc::default();
    let mut cx = gpui::VisualTestAppContext::new(current_platform(false));
    fonts::register_bundled_fonts(&cx.text_system().all_font_names());
    let path = net.clone();
    let tab_slot = slot.clone();
    let window = cx
        .open_offscreen_window(dims, move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            let tab = cx.new(|cx| {
                let mut tab = FileTab::new(path.clone(), FileMode::Plain, window, cx);
                tab.frozen = true;
                tab
            });
            *tab_slot.borrow_mut() = Some(tab.clone());
            let scene: AnyView = tab.into();
            cx.new(|_| Root { theme, scene })
        })
        .expect("open an off-screen window");
    cx.run_until_parked();
    let window: AnyWindowHandle = window.into();
    let tab = slot.borrow().clone().expect("the tab");
    click_preview(&mut cx, window, &tab);
    let started = Instant::now();
    let mut drawn = (0, Vec::new());
    while started.elapsed() < Duration::from_secs(10) {
        NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.05));
        frame(&mut cx, window);
        drawn = cx.read(|cx| tab.read(cx).mermaid_drawn());
        if drawn.0 + drawn.1.len() == 1 {
            break;
        }
    }
    // A little longer, for any late request.
    for _ in 0..10 {
        NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.05));
    }
    println!("ui-shots: file-tab-md-mermaid-no-network: {drawn:?}");
    match listener.accept() {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        other => panic!("no-network: the diagram reached the local server: {other:?}"),
    }
    println!("ui-shots: file-tab-md-mermaid-no-network: no connection");
    assert_eq!(drawn.0, 1, "no-network: the diagram renders ({drawn:?})");
    // Review fix: the page runs no script of its own (an inline handler, an inserted script).
    let mut probe = cx
        .update(Mermaids::probe_page_script)
        .expect("the web view is up");
    let started = Instant::now();
    let answer = loop {
        NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.05));
        if let Ok(Some(answer)) = probe.try_recv() {
            break answer;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "no-script: no answer"
        );
    };
    println!("ui-shots: file-tab-md-mermaid-no-script: {answer:?}");
    assert_eq!(
        answer.as_deref(),
        Ok(r#"{"handler":false,"script":false}"#),
        "no-script: the page ran a script of its own"
    );
    // Review fix: after a minute with nothing to render, the web view (and its WebKit
    // process) goes.
    cx.advance_clock(Duration::from_secs(61));
    cx.run_until_parked();
    assert!(
        cx.update(Mermaids::probe_page_script).is_none(),
        "idle: the web view is let go after 60 s"
    );
    println!("ui-shots: file-tab-md-mermaid-idle: the web view went after 60 s idle");
    std::mem::forget(cx);

    // Re-review fix: a cache of two entries, so drawing the fixture (two diagrams, then the
    // broken one) frees the first image. Painted and
    // captured after that (no stale tile: every window was refreshed), each dropped block says
    // why, and when the pause ends the windows draw again and ask for it again.
    let theme = shot_theme("Catppuccin Mocha");
    let slot: TabSlot = Rc::default();
    let mut cx = gpui::VisualTestAppContext::new(current_platform(false));
    fonts::register_bundled_fonts(&cx.text_system().all_font_names());
    cx.update(|cx| Mermaids::set_limits(2, crate::mermaid::CACHE_BYTES, cx));
    let build = build_for(theme, slot.clone());
    let window = cx
        .open_offscreen_window(dims, move |window, cx| {
            let scene = build(window, cx);
            cx.new(|_| Root { theme, scene })
        })
        .expect("open an off-screen window");
    cx.run_until_parked();
    let window: AnyWindowHandle = window.into();
    let tab = slot.borrow().clone().expect("the tab");
    click_preview(&mut cx, window, &tab);
    let started = Instant::now();
    let mut drawn = (0, Vec::new());
    while started.elapsed() < Duration::from_secs(30) {
        NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.05));
        frame(&mut cx, window);
        drawn = cx.read(|cx| tab.read(cx).mermaid_drawn());
        let (_, _, queued, running) = cx.update(Mermaids::counts);
        if drawn.0 + drawn.1.len() == 3 && queued == 0 && !running {
            break;
        }
    }
    let _ = cx
        .capture_screenshot(window)
        .expect("painted after images were freed");
    println!("ui-shots: file-tab-md-mermaid-evict: {drawn:?}");
    let crowded = crate::mermaid::note(crate::mermaid::CROWDED);
    assert_eq!(drawn.0, 1, "evict: one diagram fits ({drawn:?})");
    assert!(
        drawn.1.contains(&crowded),
        "evict: one says the cache is full ({drawn:?})"
    );
    assert!(
        drawn
            .1
            .iter()
            .any(|n| n.starts_with("Mermaid: Parse error")),
        "evict: the broken one keeps its own error ({drawn:?})"
    );
    // The pause ends: a redraw of every window is asked for (the harness draws only when
    // told, so the frame is drawn here), and the dropped diagram is asked for and queued.
    let redraws = cx.update(Mermaids::pause_redraws);
    // The store's pause is in real time (the timer in the test clock): wait both out.
    let waited = Instant::now();
    while waited.elapsed() < crate::mermaid::REDRAW_PAUSE {
        NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.1));
    }
    cx.advance_clock(crate::mermaid::REDRAW_PAUSE + Duration::from_secs(1));
    cx.run_until_parked();
    assert!(
        cx.update(Mermaids::pause_redraws) > redraws,
        "evict: the pause's end asks every window to draw again"
    );
    frame(&mut cx, window);
    let (_, _, queued, running) = cx.update(Mermaids::counts);
    assert!(
        queued > 0 || running,
        "evict: after the pause the dropped diagram is asked for again"
    );
    println!("ui-shots: file-tab-md-mermaid-evict: after the pause it renders again");
    std::mem::forget(cx);
}
