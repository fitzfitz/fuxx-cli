//! M11ai: the Markdown reader in the file tab's Preview: its centred column, type and spacing
//! (Task 3), on `READER_FIXTURE`, a made-up "Setup guide" the later tasks' scenes read too.

use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "file-tab-md-reader-column",
        run: file_tab_md_reader_column_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/file_tab_preview.rs",
            "src/md_preview.rs",
            "src/md_reader.rs",
            "src/agent_markdown.rs",
            "src/highlight.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-tab-md-reader-code",
        run: file_tab_md_reader_code_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/file_tab_preview.rs",
            "src/md_preview.rs",
            "src/md_reader.rs",
            "src/agent_markdown.rs",
            "src/highlight.rs",
            "src/tab_bar.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-tab-md-reader-callouts",
        run: file_tab_md_reader_callout_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/file_tab_preview.rs",
            "src/md_preview.rs",
            "src/md_reader.rs",
            "src/agent_markdown.rs",
            "src/tab_bar.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-tab-md-reader-github",
        run: file_tab_md_reader_github_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/file_tab_preview.rs",
            "src/md_preview.rs",
            "src/md_reader.rs",
            "src/agent_markdown.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-tab-md-reader-anchors",
        run: file_tab_md_reader_anchor_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/file_tab.rs",
            "src/file_tab_preview.rs",
            "src/md_preview.rs",
            "src/md_reader.rs",
            "src/agent_markdown.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-tab-md-reader-outline",
        run: file_tab_md_reader_outline_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/file_tab_preview.rs",
            "src/md_preview.rs",
            "src/md_reader.rs",
            "src/agent_markdown.rs",
            "src/sidebar_toggle.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-tab-md-reader-narrow",
        run: file_tab_md_reader_narrow_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/file_tab_preview.rs",
            "src/md_preview.rs",
            "src/md_reader.rs",
            "src/agent_markdown.rs",
            "src/overlay_panel.rs",
            "src/tab_bar.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-tab-md-reader-progress",
        run: file_tab_md_reader_progress_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/file_tab_preview.rs",
            "src/md_preview.rs",
            "src/md_reader.rs",
            "src/agent_markdown.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-tab-md-reader-reload",
        run: file_tab_md_reader_reload_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/file_tab.rs",
            "src/file_tab_preview.rs",
            "src/md_preview.rs",
            "src/md_reader.rs",
            "src/agent_markdown.rs",
            "src/highlight.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-tab-md-reader-big",
        run: file_tab_md_reader_big_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/file_tab.rs",
            "src/file_tab_preview.rs",
            "src/md_preview.rs",
            "src/md_reader.rs",
            "src/agent_markdown.rs",
            "src/ui_tokens.rs",
        ],
    },
];

/// A made-up "Setup guide": front matter, two badge rows, a title, sections and a subsection,
/// every callout kind, code in `sh` (one line indented with a tab), `ts`, `rust` and none, a
/// 5-column table with a long cell, a Mermaid block, and a long last section.
pub(super) const READER_FIXTURE: &str = r#"---
title: Setup guide
description: "How to install, configure and run my-app"
tags: [setup, install, config]
owner: docs-team
---

[![build: passing](https://img.shields.io/badge/build-passing-green)](https://example.com/ci) [![license: MIT](https://img.shields.io/badge/license-MIT-blue)](https://example.com/license)

![version: 1.4.2](https://img.shields.io/badge/version-1.4.2-blue) ![docs](https://example.com/docs/badge.svg)

# Setup guide

This guide walks you through installing my-app on your machine, configuring it for your project and fixing the problems people run into most often, so you can get from a fresh checkout to a running build in a few minutes.

Read it once from the top.

## Install

Clone the repository into `~/code/my-app` and run the installer:

```sh
git clone https://example.com/my-app.git ~/code/my-app
cd ~/code/my-app
	./install.sh --prefix ~/.local
```

> [!NOTE]
> The installer needs a network connection the first time it runs.

> [!TIP]
> Run `./install.sh --help` to see every option.

## Configure

The settings live in `config.ts` at the root of the project:

```ts
export const config = { port: 8080, host: "localhost", logLevel: "info", cacheDirectory: "~/.cache/my-app/build" };
```

> [!IMPORTANT]
> Restart the server after you change the port.

### Options

| Option | Type | Default | Since | Description |
|:--|:--|:--|:-:|:--|
| `port` | number | 8080 | 1.0 | The port the development server listens on; pick another one when something else on your machine already uses it |
| `host` | string | localhost | 1.0 | Where it listens |
| `logLevel` | string | info | 1.2 | How much it logs |

```rust
fn main() {
    let config = my_app::Config::load("config.ts");
    my_app::serve(config);
}
```

```mermaid
flowchart LR
  A[Install] --> B[Configure]
  B --> C[Run]
```

> [!WARNING]
> Do not commit your local settings.

> [!CAUTION]
> Deleting the cache folder removes every saved build.

## Troubleshooting

```
error: port 8080 is already in use
```

When the server will not start, the port it wants is usually taken by another copy of my-app that is still running in a terminal you forgot about. Stop that copy, or set another port in `config.ts`, and start the server again.

If the build fails right after an update, the cache may hold files from the old version. Remove the cache folder and build again; the first build after that takes a little longer while the cache fills up again.

- Check that the installer finished without errors.
- Check that `~/.local/bin` is on your path.
- Check that the port in `config.ts` is free.

When none of that helps, run the server with `logLevel: "debug"` and read the last lines it prints before it stops. They name the file or setting it could not read, which is almost always the thing to fix.
"#;

/// A 70-character English sentence: the prose column (`reader::MEASURE`) is about this wide in
/// the UI font at `reader::BODY`.
const SEVENTY: &str = "Clone the repository, run the installer, then open the project folder.";

/// M11ai Task 3's reader on `READER_FIXTURE` (SETUP.md), in both themes:
/// - `file-tab-md-reader-column` (1,200 × 700): Preview on. Every prose row (paragraphs,
///   headings, lists, quotes, callouts, badges) is at most `MEASURE` wide and centred in the
///   reader's area (the list less `TERMINAL_PADDING` each side, ±1 pt); every code block and the
///   table at least the column wide and inside the area (Task 8: less "On this page"'s column
///   and gap, which show here); the long paragraph fills the column;
///   the table is wider than it; a one-line paragraph is one 16 pt line (16 × 1.65 high). The
///   70-character sentence shaped in the UI font at 16 pt is within 8 % of `MEASURE`.
/// - `file-tab-md-reader-column-narrow` (500 × 700): nothing drawn past the area's right edge
///   (the table scrolls inside itself).
///
/// Every row is reached by scrolling the list a screen at a time (only rows in sight are drawn).
fn file_tab_md_reader_column_scenes() {
    use crate::agent_markdown::Md;
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use crate::md_reader::{BlockLook, RowKind};
    use crate::ui_tokens::reader::{BODY, BODY_LINE, MEASURE, OUTLINE_GAP, OUTLINE_W};
    use crate::ui_tokens::size::TERMINAL_PADDING;
    use gpui::{Bounds, ListOffset, Pixels, SharedString};
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-md-reader");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let guide = dir.join("SETUP.md");
    std::fs::write(&guide, READER_FIXTURE).expect("SETUP.md");

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    // Every row's content as drawn: the list scrolled a screen at a time, then back to the top.
    let walk = move |cx: &mut VisualTestAppContext, window, tab: &gpui::Entity<FileTab>| {
        let list = cx.read(|cx| tab.read(cx).preview_list());
        let rows = list.item_count();
        let mut all: BTreeMap<usize, Bounds<Pixels>> = BTreeMap::new();
        let mut next = 0;
        while next < rows {
            list.scroll_to(ListOffset {
                item_ix: next,
                offset_in_item: px(0.0),
            });
            reader_frame(cx, window);
            let drawn = cx.read(|cx| tab.read(cx).reader_drawn());
            // Each row's content ends above the next row's (its row was measured as drawn).
            for ((a, above), (b, below)) in drawn.iter().zip(drawn.iter().skip(1)) {
                assert!(
                    above.bottom() <= below.top() + px(0.5),
                    "row {a} ends at {:?}, under row {b}'s top {:?}",
                    above.bottom(),
                    below.top()
                );
            }
            let last = drawn.keys().max().copied().unwrap_or(next);
            all.extend(drawn);
            next = last.max(next) + 1;
        }
        list.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        });
        reader_frame(cx, window);
        assert_eq!(all.len(), rows, "every row drawn once: {:?}", all.keys());
        all
    };
    let (side, _) = TERMINAL_PADDING;
    // Each paragraph's row is as tall as its text needs at the width it is drawn (at least as
    // many lines as its text, shaped on one line in the UI font, is wider than its box): a row
    // measured at another width than it is drawn at overlaps the next.
    let tall_enough = move |cx: &mut VisualTestAppContext,
                            window: AnyWindowHandle,
                            tab: &gpui::Entity<FileTab>,
                            all: &BTreeMap<usize, Bounds<Pixels>>,
                            n: &str| {
        let doc = cx
            .read(|cx| tab.read(cx).reader_doc())
            .expect("the reader's document");
        for (&row, b) in all {
            let RowKind::Block(i) = doc.plan.row_kind(row) else {
                continue;
            };
            let Md::Para(spans) = &doc.blocks[i] else {
                continue;
            };
            if doc.plan.looks[i] != BlockLook::Plain {
                continue;
            }
            let text = crate::md_preview::inline(spans).text;
            let one_line = cx
                .update_window(window, |_, window, _| {
                    let run = window.text_style().to_run(text.len());
                    let line = window.text_system().shape_line(
                        SharedString::from(text.clone()),
                        px(BODY),
                        &[run],
                        None,
                    );
                    f32::from(line.width)
                })
                .expect("shape");
            let lines = (one_line / f32::from(b.size.width)).ceil().max(1.0);
            let high = f32::from(b.size.height);
            assert!(
                high >= lines * BODY * BODY_LINE - 1.0,
                "{n}: row {row} is {high} high, too short for {lines} lines of its text \
                 ({one_line} wide on one line, {} wide here)",
                f32::from(b.size.width)
            );
        }
    };

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let build_for = |slot: TabSlot| {
            let path = guide.clone();
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

        // 1,200 pt wide: the column, centred; code and the table wider.
        let n = format!("file-tab-md-reader-column-{slug}");
        let dims = size(px(1200.0), px(700.0));
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let doc = cx
                    .read(|cx| tab.read(cx).reader_doc())
                    .expect("the reader's document");
                let list = cx.read(|cx| tab.read(cx).preview_list());
                assert_eq!(doc.plan.front_rows(), 1, "{n}: the front matter's row");
                assert_eq!(list.item_count(), doc.plan.rows(), "{n}: a row each");
                let all = walk(cx, window, &tab);
                tall_enough(cx, window, &tab, &all, &n);
                let view = list.viewport_bounds();
                // Task 8: the guide's "On this page" column and its gap are not the reader's.
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_shown()),
                    Some(true),
                    "{n}: the outline's column shows"
                );
                let (left, right) = (
                    f32::from(view.left()) + side,
                    f32::from(view.right()) - side - OUTLINE_W - OUTLINE_GAP,
                );
                let (width, middle) = (right - left, (left + right) / 2.0);
                let column = MEASURE.min(width);
                let mut widest_para = 0.0f32;
                let mut table = None;
                let mut one_line = None;
                for (&row, b) in &all {
                    let RowKind::Block(i) = doc.plan.row_kind(row) else {
                        continue;
                    };
                    let md = &doc.blocks[i];
                    let (l, r, w) = (
                        f32::from(b.left()),
                        f32::from(b.right()),
                        f32::from(b.size.width),
                    );
                    let wide = matches!(md, Md::Code { .. } | Md::Table { .. });
                    if wide {
                        assert!(
                            w >= column - 0.5 && l >= left - 0.5 && r <= right + 0.5,
                            "{n}: row {row} ({md:?}) is {l}..{r}, not at least {column} wide inside {left}..{right}"
                        );
                    } else {
                        assert!(
                            w <= MEASURE + 0.5 && ((l + r) / 2.0 - middle).abs() <= 1.0,
                            "{n}: row {row} ({md:?}) is {l}..{r}, not at most {MEASURE} wide centred on {middle}"
                        );
                    }
                    match md {
                        Md::Table { .. } => table = Some(w),
                        Md::Para(spans) if doc.plan.looks[i] == BlockLook::Plain => {
                            widest_para = widest_para.max(w);
                            if crate::md_preview::inline(spans).text == "Read it once from the top."
                            {
                                one_line = Some(f32::from(b.size.height));
                            }
                        }
                        _ => {}
                    }
                }
                println!(
                    "ui-shots: {n}: area {left}..{right}, widest paragraph {widest_para}, table {table:?}"
                );
                assert!(
                    (widest_para - MEASURE).abs() <= 1.0,
                    "{n}: the long paragraph fills the column ({widest_para}, {MEASURE})"
                );
                let table = table.expect("the table is drawn");
                assert!(
                    table > widest_para + 1.0,
                    "{n}: the table ({table}) is wider than the prose ({widest_para})"
                );
                let one_line = one_line.expect("the one-line paragraph");
                assert!(
                    (one_line - BODY * BODY_LINE).abs() <= 1.0,
                    "{n}: a line of body text is {one_line} high, not {BODY} x {BODY_LINE}"
                );
                // About 70 characters of the UI font at the body size fill the column.
                let shaped = cx
                    .update_window(window, |_, window, _| {
                        let run = window.text_style().to_run(SEVENTY.len());
                        let line = window.text_system().shape_line(
                            SharedString::from(SEVENTY),
                            px(BODY),
                            &[run],
                            None,
                        );
                        f32::from(line.width)
                    })
                    .expect("shape");
                println!(
                    "ui-shots: {n}: {} characters at {BODY} pt are {shaped} wide; MEASURE {MEASURE}",
                    SEVENTY.chars().count()
                );
                assert!(
                    (shaped - MEASURE).abs() <= MEASURE * 0.08,
                    "{n}: 70 characters are {shaped} wide, not within 8 % of {MEASURE}"
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-column",
            slug,
            &theme,
            dims,
            build_for(slot.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        // The same, scrolled to the Options table (shot only: the table and the code round it).
        let dims = size(px(1200.0), px(700.0));
        let slot: TabSlot = Rc::default();
        let act = {
            let slot = slot.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let doc = cx
                    .read(|cx| tab.read(cx).reader_doc())
                    .expect("the reader's document");
                let at = doc
                    .blocks
                    .iter()
                    .position(|b| matches!(b, Md::Table { .. }))
                    .expect("the table");
                let list = cx.read(|cx| tab.read(cx).preview_list());
                list.scroll_to(ListOffset {
                    item_ix: doc.plan.row_of_block(at).saturating_sub(2),
                    offset_in_item: px(0.0),
                });
                reader_frame(cx, window);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-column-table",
            slug,
            &theme,
            dims,
            build_for(slot.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        // 500 pt wide: nothing past the area's right edge.
        let n = format!("file-tab-md-reader-column-narrow-{slug}");
        let dims = size(px(500.0), px(700.0));
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let all = walk(cx, window, &tab);
                tall_enough(cx, window, &tab, &all, &n);
                let view = cx.read(|cx| tab.read(cx).preview_list()).viewport_bounds();
                let (left, right) = (
                    f32::from(view.left()) + side,
                    f32::from(view.right()) - side,
                );
                for (row, b) in &all {
                    let (l, r) = (f32::from(b.left()), f32::from(b.right()));
                    assert!(
                        l >= left - 0.5 && r <= right + 0.5,
                        "{n}: row {row} drawn at {l}..{r}, past the area {left}..{right}"
                    );
                }
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-column-narrow",
            slug,
            &theme,
            dims,
            build_for(slot.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// M11ai Task 4's code blocks on `READER_FIXTURE` (SETUP.md), 1,200 × 700, in both themes:
/// - `file-tab-md-reader-code`: once parked, the `sh`, `ts` and `rust` blocks are coloured
///   (`code_coloured`); each code block's header names its fence's language exactly (none for
///   the block without one), its button reads `Copy`; the Mermaid block, shown as code (no
///   WebKit here), has a `mermaid` header. Inside the `rust` block's bounds some pixels are the
///   theme's keyword colour (magenta at 3:1 on the page, then at the text floor on the code's
///   fill). Copy on the `sh` block: the clipboard holds its text with its tab, the button reads
///   `Copied` until `reader::COPIED` has passed since the last press (a second press 1 s after
///   the first keeps it 1.5 s from then), then `Copy`. Shot scrolled to the `rust` block.
/// - `file-tab-md-reader-code-copied`: Copy pressed on the `sh` block; shot showing `Copied`.
fn file_tab_md_reader_code_scenes() {
    use super::branch::settle_until;
    use crate::agent_markdown::Md;
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use crate::md_preview::{Colours, Copied};
    use crate::md_reader::fence_lang;
    use crate::ui_tokens::reader::COPIED;
    use crate::ui_tokens::{MARK_MIN, TEXT_MIN, ensure_contrast};
    use gpui::{ListOffset, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-md-reader-code");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let guide = dir.join("SETUP.md");
    std::fs::write(&guide, READER_FIXTURE).expect("SETUP.md");

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    // Preview on, then frames until the colour job's answer is in (the syntaxes load and the
    // blocks colour on the background executor).
    let preview = move |cx: &mut VisualTestAppContext,
                        window: AnyWindowHandle,
                        tab: &gpui::Entity<FileTab>,
                        n: &str| {
        reader_preview_on(cx, window, tab);
        // The fixture's coloured blocks: `sh`, `ts` and `rust`.
        let coloured = settle_until(cx, window, 20, |cx| {
            cx.read(|cx| tab.read(cx).code_coloured()) == 3
        });
        let got = cx.read(|cx| tab.read(cx).code_coloured());
        assert!(coloured, "{n}: 3 code blocks coloured, not {got}");
    };
    // The list scrolled to `row`'s top, drawn.
    let show = move |cx: &mut VisualTestAppContext,
                     window: AnyWindowHandle,
                     tab: &gpui::Entity<FileTab>,
                     row: usize| {
        let list = cx.read(|cx| tab.read(cx).preview_list());
        list.scroll_to(ListOffset {
            item_ix: row,
            offset_in_item: px(0.0),
        });
        reader_frame(cx, window);
    };
    // Each code block: its row and fence language.
    let code_rows = |tab: &gpui::Entity<FileTab>, cx: &mut VisualTestAppContext| {
        let doc = cx
            .read(|cx| tab.read(cx).reader_doc())
            .expect("the reader's document");
        doc.blocks
            .iter()
            .enumerate()
            .filter_map(|(b, md)| match md {
                Md::Code { lang, text } => Some((
                    doc.plan.row_of_block(b),
                    fence_lang(lang.as_deref()).map(str::to_string),
                    text.clone(),
                )),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    // A press on row `row`'s Copy.
    let press_copy = move |cx: &mut VisualTestAppContext,
                           window: AnyWindowHandle,
                           tab: &gpui::Entity<FileTab>,
                           row: usize,
                           n: &str| {
        let button = cx
            .read(|cx| tab.read(cx).copy_bounds(row))
            .unwrap_or_else(|| panic!("{n}: row {row}'s Copy is drawn"));
        cx.simulate_click(window, button.center(), Modifiers::none());
        reader_frame(cx, window);
    };
    let label = |cx: &mut VisualTestAppContext, tab: &gpui::Entity<FileTab>, row: usize| {
        cx.read(|cx| tab.read(cx).code_drawn())
            .get(&row)
            .map(|p| p.label)
    };

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let build_for = |slot: TabSlot| {
            let path = guide.clone();
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
        let dims = size(px(1200.0), px(700.0));

        let n = format!("file-tab-md-reader-code-{slug}");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                preview(cx, window, &tab, &n);
                let rows = code_rows(&tab, cx);
                let langs: Vec<Option<&str>> = rows.iter().map(|(_, l, _)| l.as_deref()).collect();
                assert_eq!(
                    langs,
                    [Some("sh"), Some("ts"), Some("rust"), Some("mermaid"), None],
                    "{n}: the fixture's code blocks"
                );
                // Each header names its fence's language as written (none: no words), and
                // offers Copy.
                for (row, lang, _) in &rows {
                    show(cx, window, &tab, *row);
                    let drawn = cx.read(|cx| tab.read(cx).code_drawn());
                    let probe = drawn
                        .get(row)
                        .unwrap_or_else(|| panic!("{n}: row {row}'s header is drawn"));
                    assert_eq!(&probe.lang, lang, "{n}: row {row}'s header");
                    assert_eq!(probe.label, "Copy", "{n}: row {row}'s button");
                }
                // The Mermaid block is shown as code (no WebKit in a plain scene).
                let (diagrams, _) = cx.read(|cx| tab.read(cx).mermaid_drawn());
                assert_eq!(diagrams, 0, "{n}: no diagram drawn here");

                // Copy on the `sh` block (its third line starts with a tab).
                let (sh_row, _, sh_text) = rows[0].clone();
                assert!(sh_text.contains('\t'), "{n}: the fixture's tab");
                show(cx, window, &tab, sh_row);
                press_copy(cx, window, &tab, sh_row, &n);
                let clip = cx.read_from_clipboard().and_then(|c| c.text());
                assert_eq!(
                    clip.as_deref(),
                    Some(sh_text.as_str()),
                    "{n}: the code copied"
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).copied()),
                    Some(Copied::Code(sh_row))
                );
                assert_eq!(label(cx, &tab, sh_row), Some("Copied"), "{n}");
                // Pressed again 1 s on: "Copied" lasts `COPIED` from the second press.
                cx.advance_clock(Duration::from_millis(1_000));
                reader_frame(cx, window);
                press_copy(cx, window, &tab, sh_row, &n);
                cx.advance_clock(COPIED - Duration::from_millis(500));
                reader_frame(cx, window);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).copied()),
                    Some(Copied::Code(sh_row)),
                    "{n}: still Copied 1 s after the second press"
                );
                cx.advance_clock(Duration::from_millis(600));
                reader_frame(cx, window);
                assert_eq!(cx.read(|cx| tab.read(cx).copied()), None, "{n}: Copy again");
                assert_eq!(label(cx, &tab, sh_row), Some("Copy"), "{n}");

                // The `rust` block's keyword colour, inside its bounds.
                let (rust_row, _, _) = rows[2].clone();
                show(cx, window, &tab, rust_row);
                let bounds = *cx
                    .read(|cx| tab.read(cx).reader_drawn())
                    .get(&rust_row)
                    .expect("the rust block is drawn");
                let fill = Colours::of(&theme).code_bg;
                let keyword = ensure_contrast(
                    ensure_contrast(theme.palette[5], theme.background, MARK_MIN),
                    fill,
                    TEXT_MIN,
                );
                assert_ne!(keyword, Colours::of(&theme).code_text, "{n}");
                let shot = frame_now(cx, window);
                let (mut hits, mut y) = (0, f32::from(bounds.top()));
                while y < f32::from(bounds.bottom()).min(700.0) {
                    let mut x = f32::from(bounds.left());
                    while x < f32::from(bounds.right()) {
                        if gap(pixel_at(&shot, dims, x, y), keyword) <= 6 {
                            hits += 1;
                        }
                        x += 0.5;
                    }
                    y += 0.5;
                }
                println!("ui-shots: {n}: {hits} keyword pixels in the rust block");
                assert!(
                    hits >= 4,
                    "{n}: the rust block shows its keyword colour ({hits})"
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-code",
            slug,
            &theme,
            dims,
            build_for(slot.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        let n = format!("file-tab-md-reader-code-copied-{slug}");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                preview(cx, window, &tab, &n);
                let (sh_row, _, _) = code_rows(&tab, cx)[0].clone();
                show(cx, window, &tab, sh_row.saturating_sub(1));
                press_copy(cx, window, &tab, sh_row, &n);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).copied()),
                    Some(Copied::Code(sh_row))
                );
                let drawn = cx.read(|cx| tab.read(cx).code_drawn());
                let probe = drawn.get(&sh_row).expect("the sh block's header");
                assert_eq!(probe.label, "Copied", "{n}: the button reads Copied");
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-code-copied",
            slug,
            &theme,
            dims,
            build_for(slot.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// M11ai Task 5's callouts on `READER_FIXTURE` plus a `[!FOO]` quote (SETUP.md), 1,200 × 700, in
/// both themes, `file-tab-md-reader-callouts`: each of the five callouts, scrolled to, draws its
/// title word (`callout_drawn`), a pixel of the kind's bar colour just inside the block's left
/// edge (mid height), the kind's wash a little right of the bar; and the `[!FOO]` quote is still
/// a quote: no title, the dim `line` bar at its left edge, no wash. Shot scrolled to the `Note`.
fn file_tab_md_reader_callout_scenes() {
    use crate::agent_markdown::Md;
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use crate::md_preview::Colours;
    use crate::md_reader::{BlockLook, Callout};
    use gpui::{ListOffset, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-md-reader-callouts");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let guide = dir.join("SETUP.md");
    std::fs::write(
        &guide,
        format!("{READER_FIXTURE}\n> [!FOO]\n> Not an alert.\n"),
    )
    .expect("SETUP.md");

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let dims = size(px(1200.0), px(700.0));
        let n = format!("file-tab-md-reader-callouts-{slug}");
        let slot: TabSlot = Rc::default();
        let build = {
            let (path, slot) = (guide.clone(), slot.clone());
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
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = slot.borrow().clone().expect("the file tab");
                reader_frame(cx, window);
                let button = cx
                    .read(|cx| tab.read(cx).preview_bounds())
                    .expect("Preview is drawn");
                cx.simulate_click(window, button.center(), Modifiers::none());
                reader_frame(cx, window);
                let doc = cx
                    .read(|cx| tab.read(cx).reader_doc())
                    .expect("the reader's document");
                let colours = Colours::of(&theme);
                let show = |cx: &mut VisualTestAppContext, row: usize| {
                    cx.read(|cx| tab.read(cx).preview_list())
                        .scroll_to(ListOffset {
                            item_ix: row,
                            offset_in_item: px(0.0),
                        });
                    reader_frame(cx, window);
                };
                let mut seen = Vec::new();
                for (b, look) in doc.plan.looks.iter().enumerate() {
                    let row = doc.plan.row_of_block(b);
                    if let BlockLook::Callout(kind, _) = look {
                        seen.push(*kind);
                        show(cx, row);
                        let title = cx
                            .read(|cx| tab.read(cx).callout_drawn())
                            .get(&row)
                            .copied();
                        assert_eq!(title, Some(kind.title()), "{n}: row {row}'s title");
                        let bounds = *cx
                            .read(|cx| tab.read(cx).reader_drawn())
                            .get(&row)
                            .unwrap_or_else(|| panic!("{n}: row {row} is drawn"));
                        let c = colours.callout(*kind);
                        let shot = frame_now(cx, window);
                        let y = f32::from(bounds.center().y);
                        let bar = pixel_at(&shot, dims, f32::from(bounds.left()) + 1.0, y);
                        assert!(
                            gap(bar, c.bar) <= 6,
                            "{n}: {} bar {bar:?} is not {:?}",
                            kind.title(),
                            c.bar
                        );
                        let wash = pixel_at(&shot, dims, f32::from(bounds.left()) + 8.0, y);
                        assert!(
                            gap(wash, c.wash) <= 6,
                            "{n}: {} wash {wash:?} is not {:?}",
                            kind.title(),
                            c.wash
                        );
                    }
                }
                assert_eq!(
                    seen,
                    Callout::ALL.to_vec(),
                    "{n}: the five callouts, in order"
                );

                // The unknown marker stays a quote: its text still shows `[!FOO]`.
                let odd = doc
                    .blocks
                    .iter()
                    .position(|md| matches!(md, Md::Quote(_)))
                    .map(|_| doc.blocks.len() - 1)
                    .expect("the [!FOO] quote");
                assert!(
                    matches!(doc.blocks[odd], Md::Quote(_)),
                    "{n}: [!FOO] is a quote"
                );
                assert_eq!(
                    doc.plan.looks[odd],
                    BlockLook::Plain,
                    "{n}: [!FOO] is plain"
                );
                let row = doc.plan.row_of_block(odd);
                show(cx, row);
                assert!(
                    !cx.read(|cx| tab.read(cx).callout_drawn())
                        .contains_key(&row),
                    "{n}: no callout title on the quote"
                );
                let bounds = *cx
                    .read(|cx| tab.read(cx).reader_drawn())
                    .get(&row)
                    .expect("the quote is drawn");
                let shot = frame_now(cx, window);
                let y = f32::from(bounds.center().y);
                let bar = pixel_at(&shot, dims, f32::from(bounds.left()) + 1.0, y);
                assert!(
                    gap(bar, colours.line) <= 6,
                    "{n}: the quote's bar {bar:?} is not {:?}",
                    colours.line
                );
                let beside = pixel_at(&shot, dims, f32::from(bounds.left()) + 8.0, y);
                assert!(
                    gap(beside, theme.background) <= 6,
                    "{n}: no wash behind the quote ({beside:?})"
                );

                let note = doc.plan.row_of_block(
                    doc.plan
                        .looks
                        .iter()
                        .position(|l| matches!(l, BlockLook::Callout(Callout::Note, _)))
                        .expect("the Note"),
                );
                show(cx, note);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-callouts",
            slug,
            &theme,
            dims,
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// M11ai Task 6's GitHub looks, 1,200 × 700, in both themes:
/// - `file-tab-md-reader-github` (`READER_FIXTURE`): the front matter row draws its keys and
///   values (`title` / `Setup guide` first), no `+ N more`; it sits at the top padding (its top
///   at the list's top plus `TERMINAL_PADDING.1`) and the block after it is the reader's gap
///   under it; no rule and no heading come before `# Setup guide`. Each badge row draws one
///   pill per badge, each `PILL_H` high, in order, with its words; the `license | MIT` pill has
///   two fills (`pill_bg` at its left end, `pill_value_bg` at its right), the one-part `docs`
///   pill one. A click on the linked `build` pill follows `https://example.com/ci` (recorded by
///   the frozen tab, no browser).
/// - `file-tab-md-reader-github-table`: scrolled to the Options table: down its first column's
///   padding the fills are the header's, the page (1st body row), the stripe (2nd), the page
///   (3rd).
/// - `file-tab-md-reader-github-more`: 25 front-matter keys: 20 rows, then `+ 5 more`; a badge in
///   a sentence shows the pill fill inside its paragraph; a badge with no alt text is a dim
///   `badge` pill.
/// - `file-tab-md-reader-github-raw`: front matter that is not `key: value` lines: a code block
///   with a `front matter` header and no Copy.
fn file_tab_md_reader_github_scenes() {
    use crate::agent_markdown::Md;
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use crate::md_preview::Colours;
    use crate::md_reader::{BlockLook, heading_text};
    use crate::ui_tokens::reader::{BLOCK_GAP, FRONT_ROWS, HEADING_ABOVE, PILL_H};
    use crate::ui_tokens::size::TERMINAL_PADDING;
    use gpui::{ListOffset, Modifiers};
    use std::cell::RefCell;
    use std::fmt::Write as _;
    use std::rc::Rc;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-md-reader-github");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let guide = dir.join("SETUP.md");
    std::fs::write(&guide, READER_FIXTURE).expect("SETUP.md");
    let mut many = String::from("---\n");
    for i in 1..=FRONT_ROWS + 5 {
        writeln!(many, "key{i}: value {i}").expect("a line");
    }
    many.push_str(
        "---\n\nThe build is ![build: passing](https://img.shields.io/badge/build-passing-green) \
         on the main branch.\n\n![](https://img.shields.io/badge/coverage-90-green) \
         ![docs](https://example.com/docs/badge.svg)\n",
    );
    let more = dir.join("MANY.md");
    std::fs::write(&more, many).expect("MANY.md");
    let raw = dir.join("RAW.md");
    std::fs::write(
        &raw,
        "---\nThis is a note, not a list of settings.\n---\n\n# Notes\n\nSome text.\n",
    )
    .expect("RAW.md");

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    let show = move |cx: &mut VisualTestAppContext,
                     window: AnyWindowHandle,
                     tab: &gpui::Entity<FileTab>,
                     row: usize| {
        cx.read(|cx| tab.read(cx).preview_list())
            .scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.0),
            });
        reader_frame(cx, window);
    };

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let colours = Colours::of(&theme);
        let dims = size(px(1200.0), px(700.0));
        let build_for = |slot: TabSlot, path: std::path::PathBuf| {
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

        // The fixture's top: the front matter and the badge rows.
        let n = format!("file-tab-md-reader-github-{slug}");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let doc = cx
                    .read(|cx| tab.read(cx).reader_doc())
                    .expect("the reader's document");
                assert_eq!(doc.plan.front_rows(), 1, "{n}: the front matter's row");
                let front = cx
                    .read(|cx| tab.read(cx).front_drawn())
                    .unwrap_or_else(|| panic!("{n}: the front matter is drawn"));
                println!("ui-shots: {n}: front matter {front:?}");
                assert_eq!(
                    front.rows.first().map(|(k, v)| (k.as_str(), v.as_str())),
                    Some(("title", "Setup guide")),
                    "{n}: the first key and value"
                );
                assert_eq!(front.rows.len(), 4, "{n}: four keys");
                assert_eq!(front.more, None, "{n}: no + N more");
                assert_eq!(front.raw, None, "{n}: a table");
                // At the top padding; the next block the reader's gap under it.
                let view = cx.read(|cx| tab.read(cx).preview_list()).viewport_bounds();
                let drawn = cx.read(|cx| tab.read(cx).reader_drawn());
                let (first, next) = (drawn[&0], drawn[&1]);
                let top = f32::from(first.top()) - f32::from(view.top());
                assert!(
                    (top - TERMINAL_PADDING.1).abs() <= 0.5,
                    "{n}: the front matter's top is {top} under the list's, not {}",
                    TERMINAL_PADDING.1
                );
                assert!(
                    (f32::from(front.bounds.top()) - f32::from(first.top())).abs() <= 0.5
                        && front.bounds.size.height > px(30.0),
                    "{n}: the row is the table ({:?}, row {first:?})",
                    front.bounds
                );
                let want = match doc.blocks.first() {
                    Some(Md::Heading(..)) => HEADING_ABOVE,
                    _ => BLOCK_GAP,
                };
                let between = f32::from(next.top()) - f32::from(first.bottom());
                assert!(
                    (between - want).abs() <= 0.5,
                    "{n}: {between} between the front matter and the next block, not {want}"
                );
                // No rule or heading before `# Setup guide` (the front matter is not text).
                let h1 = doc
                    .blocks
                    .iter()
                    .position(|b| matches!(b, Md::Heading(..)))
                    .expect("a heading");
                assert!(
                    matches!(&doc.blocks[h1], Md::Heading(1, s) if heading_text(s) == "Setup guide"),
                    "{n}: the first heading is the title"
                );
                assert!(
                    !doc.blocks[..h1].iter().any(|b| matches!(b, Md::Rule)),
                    "{n}: no rule above the title"
                );

                // The badge rows: a pill per badge, PILL_H high, with its words.
                let pills = cx.read(|cx| tab.read(cx).pills_drawn());
                let shot = frame_now(cx, window);
                let mut rows = 0;
                for (b, look) in doc.plan.looks.iter().enumerate() {
                    let BlockLook::Badges(badges) = look else {
                        continue;
                    };
                    rows += 1;
                    let row = doc.plan.row_of_block(b);
                    let got: Vec<_> = pills
                        .range((row, 0)..(row + 1, 0))
                        .map(|(_, p)| p)
                        .collect();
                    assert_eq!(got.len(), badges.len(), "{n}: row {row}'s pills");
                    for (badge, pill) in badges.iter().zip(&got) {
                        assert_eq!(
                            (&pill.label, &pill.value),
                            (&badge.label, &badge.value),
                            "{n}: row {row}'s pill"
                        );
                        let h = f32::from(pill.bounds.size.height);
                        assert!((h - PILL_H).abs() <= 0.5, "{n}: {} is {h} high", pill.label);
                        let y = f32::from(pill.bounds.center().y);
                        let left = pixel_at(&shot, dims, f32::from(pill.bounds.left()) + 3.0, y);
                        let right = pixel_at(&shot, dims, f32::from(pill.bounds.right()) - 3.0, y);
                        let value = if pill.value.is_some() {
                            colours.pill_value_bg
                        } else {
                            colours.pill_bg
                        };
                        assert!(
                            gap(left, colours.pill_bg) <= 3 && gap(right, value) <= 3,
                            "{n}: {}'s fills {left:?} / {right:?}, not {:?} / {value:?}",
                            pill.label,
                            colours.pill_bg
                        );
                    }
                }
                assert_eq!(rows, 2, "{n}: two badge rows");
                assert!(
                    pills
                        .values()
                        .any(|p| p.label == "license" && p.value.as_deref() == Some("MIT")),
                    "{n}: the license | MIT pill"
                );
                // A click on the linked `build` pill follows its link (recorded, no browser).
                let build = pills
                    .values()
                    .find(|p| p.label == "build")
                    .expect("the build pill")
                    .bounds;
                cx.simulate_click(window, build.center(), Modifiers::none());
                reader_frame(cx, window);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).followed_link()).as_deref(),
                    Some("https://example.com/ci"),
                    "{n}: the build pill's link"
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-github",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), guide.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        // The Options table, striped.
        let n = format!("file-tab-md-reader-github-table-{slug}");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let doc = cx
                    .read(|cx| tab.read(cx).reader_doc())
                    .expect("the reader's document");
                let at = doc
                    .blocks
                    .iter()
                    .position(|b| matches!(b, Md::Table { .. }))
                    .expect("the table");
                let row = doc.plan.row_of_block(at);
                show(cx, window, &tab, row.saturating_sub(1));
                let table = *cx
                    .read(|cx| tab.read(cx).reader_drawn())
                    .get(&row)
                    .unwrap_or_else(|| panic!("{n}: the table is drawn"));
                let shot = frame_now(cx, window);
                // Down the first column's side padding: each pixel the nearest of the fills
                // (lines skipped), runs of 3 or more kept.
                let fills = [
                    ("header", colours.header_bg),
                    ("page", theme.background),
                    ("stripe", colours.stripe),
                    ("line", colours.line),
                ];
                let x = f32::from(table.left()) + 4.0;
                let (mut runs, mut y): (Vec<(&str, usize)>, f32) =
                    (Vec::new(), f32::from(table.top()) + 2.0);
                while y < f32::from(table.bottom()) - 2.0 {
                    let p = pixel_at(&shot, dims, x, y);
                    let (what, _) = fills
                        .iter()
                        .min_by_key(|(_, c)| gap(p, *c))
                        .copied()
                        .expect("a fill");
                    match runs.last_mut() {
                        Some((last, k)) if *last == what => *k += 1,
                        _ => runs.push((what, 1)),
                    }
                    y += 1.0;
                }
                let seen: Vec<&str> = runs
                    .iter()
                    .filter(|(what, k)| *what != "line" && *k >= 3)
                    .map(|(what, _)| *what)
                    .fold(Vec::new(), |mut v, w| {
                        if v.last() != Some(&w) {
                            v.push(w);
                        }
                        v
                    });
                println!("ui-shots: {n}: down the first column {runs:?}");
                assert_eq!(
                    seen,
                    ["header", "page", "stripe", "page"],
                    "{n}: the header, then every 2nd body row striped"
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-github-table",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), guide.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        // 25 keys: 20 rows and `+ 5 more`; a badge in a sentence.
        let n = format!("file-tab-md-reader-github-more-{slug}");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let front = cx
                    .read(|cx| tab.read(cx).front_drawn())
                    .unwrap_or_else(|| panic!("{n}: the front matter is drawn"));
                assert_eq!(front.rows.len(), FRONT_ROWS, "{n}: {FRONT_ROWS} rows");
                assert_eq!(front.more.as_deref(), Some("+ 5 more"), "{n}");
                // The sentence with a badge: the pill fill inside its paragraph.
                show(cx, window, &tab, 1);
                let para = *cx
                    .read(|cx| tab.read(cx).reader_drawn())
                    .get(&1)
                    .unwrap_or_else(|| panic!("{n}: the paragraph is drawn"));
                let shot = frame_now(cx, window);
                let y = f32::from(para.center().y);
                let mut x = f32::from(para.left());
                let mut hits = 0;
                while x < f32::from(para.right()) {
                    if gap(pixel_at(&shot, dims, x, y), colours.pill_bg) <= 2 {
                        hits += 1;
                    }
                    x += 1.0;
                }
                println!("ui-shots: {n}: {hits} pill-fill pixels across the sentence");
                assert!(
                    hits >= 20,
                    "{n}: the badge in the sentence has the pill fill ({hits})"
                );
                // An empty alt text: the word `badge`, dim.
                show(cx, window, &tab, 2);
                let pills = cx.read(|cx| tab.read(cx).pills_drawn());
                let words: Vec<(&str, bool)> = pills
                    .range((2, 0)..(3, 0))
                    .map(|(_, p)| (p.label.as_str(), p.dim))
                    .collect();
                assert_eq!(
                    words,
                    [("badge", true), ("docs", false)],
                    "{n}: the empty badge"
                );
                // Shot with the table's end, `+ 5 more`, and the rows under it in view.
                cx.read(|cx| tab.read(cx).preview_list())
                    .scroll_to(ListOffset {
                        item_ix: 0,
                        offset_in_item: front.bounds.size.height - px(160.0),
                    });
                reader_frame(cx, window);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-github-more",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), more.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        // Not `key: value` lines: a code block headed `front matter`, no Copy.
        let n = format!("file-tab-md-reader-github-raw-{slug}");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let front = cx
                    .read(|cx| tab.read(cx).front_drawn())
                    .unwrap_or_else(|| panic!("{n}: the front matter is drawn"));
                assert_eq!(front.raw, Some("front matter"), "{n}: the header");
                assert!(front.rows.is_empty(), "{n}: no table");
                assert!(
                    !cx.read(|cx| tab.read(cx).code_drawn()).contains_key(&0),
                    "{n}: no Copy on the front matter"
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-github-raw",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), raw.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// The anchor scenes' links, each a paragraph of its own (put under "Read it once from the top."
/// in `READER_FIXTURE`): a heading of this file, one that is not here, and a heading of another
/// file beside it.
const ANCHOR_LINKS: &str = "Read it once from the top.\n\n[see Configure](#configure)\n\n\
                            [a part that is not here](#nope)\n\n[the other guide](other.md#middle)\n";

/// M11ai Task 7's heading anchors (state, one theme, 1,200 × 700), in a fixture folder made a
/// git repo (`git init`; nothing committed), the guide at `docs/SETUP.md`:
/// - `file-tab-md-reader-anchors`: a click on `[see Configure](#configure)` puts the Configure
///   heading's row at the list's top, `jump_offset` into it (its text `JUMP_ROOM` under the
///   list's top), and the scrollbar shows; `#nope` changes nothing. Hovering the heading shows
///   its `#` (drawn only then), `ANCHOR_GAP` left of the text; a click on it puts
///   `docs/SETUP.md#configure` on the clipboard (without git: only the `#configure` end is
///   checked) and `copied()` is `Anchor(row)`, the `#` reading `Copied` until `reader::COPIED`
///   has passed. A click on `[the other guide](other.md#middle)` asks the workspace to open
///   `docs/other.md` at `middle`.
/// - `file-tab-md-reader-anchors-other`: a fresh tab on `docs/other.md` given that anchor
///   (`show_anchor`, before its file is read) shows its Preview with `## Middle` `JUMP_ROOM`
///   under the top once parked.
fn file_tab_md_reader_anchor_scenes() {
    use crate::agent_markdown::Md;
    use crate::file_tab::{FileTab, FileTabEvent};
    use crate::layout_model::FileMode;
    use crate::md_preview::Copied;
    use crate::md_reader::{heading_text, jump_offset};
    use crate::ui_tokens::reader::{ANCHOR_GAP, COPIED, HEADING_ABOVE, JUMP_ROOM};
    use gpui::{ListOffset, Modifiers};
    use std::cell::RefCell;
    use std::fmt::Write as _;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::Duration;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-md-reader-anchors");
    let _ = std::fs::remove_dir_all(&dir);
    let docs = dir.join("docs");
    std::fs::create_dir_all(&docs).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let docs = dir.join("docs");
    let guide = docs.join("SETUP.md");
    let fixture = READER_FIXTURE.replacen("Read it once from the top.\n", ANCHOR_LINKS, 1);
    assert_ne!(fixture, READER_FIXTURE, "the links are in");
    std::fs::write(&guide, fixture).expect("SETUP.md");
    let mut other = String::from("# Other guide\n\n");
    for i in 1..=30 {
        writeln!(
            other,
            "Paragraph {i} before the middle of the other guide.\n"
        )
        .expect("a line");
    }
    other.push_str("## Middle\n\nThe middle of the other guide.\n\n");
    for i in 1..=30 {
        writeln!(
            other,
            "Paragraph {i} after the middle of the other guide.\n"
        )
        .expect("a line");
    }
    let other_md = docs.join("other.md");
    std::fs::write(&other_md, other).expect("other.md");
    // A repo, so the copied link is from its top folder (nothing is committed in it).
    let in_repo = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .status()
        .is_ok_and(|s| s.success());
    if !in_repo {
        println!(
            "ui-shots: file-tab-md-reader-anchors: no git; the copied link's path is not checked"
        );
    }

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    type Opened = Rc<RefCell<Option<(PathBuf, Option<String>)>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    let show = move |cx: &mut VisualTestAppContext,
                     window: AnyWindowHandle,
                     tab: &gpui::Entity<FileTab>,
                     row: usize| {
        cx.read(|cx| tab.read(cx).preview_list())
            .scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.0),
            });
        reader_frame(cx, window);
    };
    // The row of the paragraph that is only the link `words`.
    let link_row = |cx: &mut VisualTestAppContext, tab: &gpui::Entity<FileTab>, words: &str| {
        let doc = cx
            .read(|cx| tab.read(cx).reader_doc())
            .expect("the reader's document");
        let b = doc
            .blocks
            .iter()
            .position(|b| matches!(b, Md::Para(s) if crate::md_preview::inline(s).text == words))
            .unwrap_or_else(|| panic!("the link {words}"));
        doc.plan.row_of_block(b)
    };
    // A click on the start of the link paragraph `words` (scrolled into sight first).
    let click_link = move |cx: &mut VisualTestAppContext,
                           window: AnyWindowHandle,
                           tab: &gpui::Entity<FileTab>,
                           words: &str| {
        let row = link_row(cx, tab, words);
        show(cx, window, tab, row.saturating_sub(1));
        let at = *cx
            .read(|cx| tab.read(cx).reader_drawn())
            .get(&row)
            .unwrap_or_else(|| panic!("the link {words} is drawn"));
        cx.simulate_click(
            window,
            point(at.left() + px(6.0), at.center().y),
            Modifiers::none(),
        );
        reader_frame(cx, window);
    };
    let heading_row = |cx: &mut VisualTestAppContext, tab: &gpui::Entity<FileTab>, words: &str| {
        let doc = cx
            .read(|cx| tab.read(cx).reader_doc())
            .expect("the reader's document");
        let b = doc
            .blocks
            .iter()
            .position(|b| matches!(b, Md::Heading(_, s) if heading_text(s) == words))
            .unwrap_or_else(|| panic!("the heading {words}"));
        doc.plan.row_of_block(b)
    };
    // The heading on `row` jumped to: the row `jump_offset` in at the list's top, its text
    // `JUMP_ROOM` under the list's top.
    let at_top =
        move |cx: &mut VisualTestAppContext, tab: &gpui::Entity<FileTab>, row: usize, n: &str| {
            let list = cx.read(|cx| tab.read(cx).preview_list());
            let top = list.logical_scroll_top();
            let want = jump_offset(HEADING_ABOVE);
            assert_eq!(
                (top.item_ix, f32::from(top.offset_in_item)),
                (row, want),
                "{n}: the heading's row, {want} in, at the top"
            );
            let drawn = *cx
                .read(|cx| tab.read(cx).reader_drawn())
                .get(&row)
                .unwrap_or_else(|| panic!("{n}: the heading is drawn"));
            let under = f32::from(drawn.top()) - f32::from(list.viewport_bounds().top());
            println!("ui-shots: {n}: row {row} at the top, its text {under} under it");
            assert!(
                (under - JUMP_ROOM).abs() <= 0.5,
                "{n}: the heading's text is {under} under the top, not {JUMP_ROOM}"
            );
        };

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let dims = size(px(1200.0), px(700.0));
        let build_for = |slot: TabSlot, opened: Opened, path: PathBuf| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(path.clone(), FileMode::Plain, window, cx);
                    tab.frozen = true;
                    tab
                });
                cx.subscribe(&tab, move |_, e: &FileTabEvent, _| {
                    if let FileTabEvent::OpenFile(path, part) = e {
                        *opened.borrow_mut() = Some((path.clone(), part.clone()));
                    }
                })
                .detach();
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };

        let n = format!("file-tab-md-reader-anchors-{slug}");
        let (slot, opened): (TabSlot, Opened) = (Rc::default(), Rc::default());
        let act = {
            let (n, slot, opened) = (n.clone(), slot.clone(), opened.clone());
            let other_md = other_md.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                // The frozen tab's place for git, looked up now (a frozen tab does not).
                cx.update(|cx| tab.update(cx, |tab, cx| tab.look_up_place_for_shots(cx)));
                reader_frame(cx, window);
                cx.update_window(window, |_, window, cx| {
                    tab.update(cx, |tab, cx| tab.choose_preview_for_shots(true, window, cx))
                })
                .expect("Preview on");
                reader_frame(cx, window);
                let configure = heading_row(cx, &tab, "Configure");

                // `#configure`: a jump.
                click_link(cx, window, &tab, "see Configure");
                assert_eq!(
                    cx.read(|cx| tab.read(cx).followed_link()).as_deref(),
                    Some("#configure"),
                    "{n}: the link clicked"
                );
                at_top(cx, &tab, configure, &n);
                assert!(
                    cx.read(|cx| tab.read(cx).scrollbar_shown()),
                    "{n}: a jump shows the scrollbar"
                );

                // `#nope`: nothing.
                click_link(cx, window, &tab, "a part that is not here");
                assert_eq!(
                    cx.read(|cx| tab.read(cx).followed_link()).as_deref(),
                    Some("#nope"),
                    "{n}: the link clicked"
                );
                let list = cx.read(|cx| tab.read(cx).preview_list());
                let before = list.logical_scroll_top();
                cx.simulate_click(window, point(px(5.0), px(5.0)), Modifiers::none());
                let nope = link_row(cx, &tab, "a part that is not here");
                let at = *cx
                    .read(|cx| tab.read(cx).reader_drawn())
                    .get(&nope)
                    .expect("the #nope link is drawn");
                cx.simulate_click(
                    window,
                    point(at.left() + px(6.0), at.center().y),
                    Modifiers::none(),
                );
                reader_frame(cx, window);
                let after = list.logical_scroll_top();
                assert_eq!(
                    (after.item_ix, after.offset_in_item),
                    (before.item_ix, before.offset_in_item),
                    "{n}: #nope does not scroll"
                );

                // Another file's heading: the workspace is asked to open it there.
                click_link(cx, window, &tab, "the other guide");
                assert_eq!(
                    opened.borrow().clone(),
                    Some((other_md.clone(), Some("middle".to_string()))),
                    "{n}: other.md#middle asked for"
                );

                // The heading's `#`: drawn only while the heading is hovered.
                click_link(cx, window, &tab, "see Configure");
                at_top(cx, &tab, configure, &n);
                let heading = *cx
                    .read(|cx| tab.read(cx).reader_drawn())
                    .get(&configure)
                    .expect("the heading is drawn");
                let para = *cx
                    .read(|cx| tab.read(cx).reader_drawn())
                    .get(&(configure + 1))
                    .expect("the paragraph under it is drawn");
                cx.simulate_mouse_move(window, para.center(), None, Modifiers::none());
                reader_frame(cx, window);
                assert!(
                    !cx.read(|cx| tab.read(cx).anchor_drawn())
                        .contains_key(&configure),
                    "{n}: no # while the heading is not hovered"
                );
                cx.simulate_mouse_move(
                    window,
                    point(heading.left() + px(20.0), heading.top() + px(10.0)),
                    None,
                    Modifiers::none(),
                );
                reader_frame(cx, window);
                let probe = cx
                    .read(|cx| tab.read(cx).anchor_drawn())
                    .get(&configure)
                    .cloned()
                    .unwrap_or_else(|| panic!("{n}: the heading's # is drawn on hover"));
                println!("ui-shots: {n}: the # {probe:?}, the heading {heading:?}");
                assert_eq!(probe.label, "#", "{n}: the #");
                let gap = f32::from(heading.left()) - f32::from(probe.bounds.right());
                assert!(
                    (gap - ANCHOR_GAP).abs() <= 1.0,
                    "{n}: the # is {gap} left of the heading, not {ANCHOR_GAP}"
                );
                assert!(
                    probe.bounds.top() >= heading.top()
                        && probe.bounds.bottom() <= heading.bottom(),
                    "{n}: the # is on the heading's line"
                );

                // A click on it: the link to the heading copied, the `#` reads Copied.
                cx.simulate_mouse_move(window, probe.bounds.center(), None, Modifiers::none());
                reader_frame(cx, window);
                cx.simulate_click(window, probe.bounds.center(), Modifiers::none());
                reader_frame(cx, window);
                let clip = cx.read_from_clipboard().and_then(|c| c.text());
                println!("ui-shots: {n}: copied {clip:?}");
                if in_repo {
                    assert_eq!(
                        clip.as_deref(),
                        Some("docs/SETUP.md#configure"),
                        "{n}: the link from the repo's top"
                    );
                } else {
                    assert!(
                        clip.as_deref()
                            .is_some_and(|c| c.ends_with("SETUP.md#configure")),
                        "{n}: the link to the heading ({clip:?})"
                    );
                }
                assert_eq!(
                    cx.read(|cx| tab.read(cx).copied()),
                    Some(Copied::Anchor(configure)),
                    "{n}: Copied"
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).anchor_drawn())
                        .get(&configure)
                        .map(|p| p.label),
                    Some("Copied"),
                    "{n}: the # reads Copied"
                );
                at_top(cx, &tab, configure, &n);
                cx.advance_clock(COPIED + Duration::from_millis(100));
                reader_frame(cx, window);
                assert_eq!(cx.read(|cx| tab.read(cx).copied()), None, "{n}: # again");
                // Shot with Copied showing.
                cx.simulate_click(window, probe.bounds.center(), Modifiers::none());
                reader_frame(cx, window);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-anchors",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), opened.clone(), guide.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        // The other file, opened at `middle` as the workspace does: before it is read.
        let n = format!("file-tab-md-reader-anchors-other-{slug}");
        let (slot, opened): (TabSlot, Opened) = (Rc::default(), Rc::default());
        let build = {
            let (inner, slot) = (
                build_for(slot.clone(), opened, other_md.clone()),
                slot.clone(),
            );
            move |window: &mut Window, cx: &mut App| -> AnyView {
                let view = inner(window, cx);
                if let Some(tab) = slot.borrow().clone() {
                    tab.update(cx, |tab, cx| tab.show_anchor("middle".to_string(), cx));
                }
                view
            }
        };
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_frame(cx, window);
                reader_frame(cx, window);
                assert!(
                    cx.read(|cx| tab.read(cx).preview_shown()),
                    "{n}: in Preview"
                );
                let middle = heading_row(cx, &tab, "Middle");
                at_top(cx, &tab, middle, &n);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-anchors-other",
            slug,
            &theme,
            dims,
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// A made-up "Field guide" for "On this page" (M11ai Task 8): a title, three paragraphs of
/// introduction, then 12 sections (`## Section 1` … `## Section 12`) of four paragraphs each;
/// section 2 has a subsection (`### Section 2 details`), section 7's heading is too long for one
/// line of the outline, and section 12 is a single short line.
fn outline_fixture() -> String {
    use std::fmt::Write as _;
    let para = |s: &str, j: usize| {
        format!(
            "Paragraph {j} of {s} of the field guide: it says what this part covers, which \
             settings it reads and what to check when something does not work as it should, so \
             a reader can follow along without the source open.\n"
        )
    };
    let mut doc = String::from("# Field guide\n\n");
    for j in 1..=3 {
        writeln!(doc, "{}", para("the introduction", j)).expect("a line");
    }
    for i in 1..=12 {
        let title = if i == 7 {
            OUTLINE_LONG.to_string()
        } else {
            format!("Section {i}")
        };
        writeln!(doc, "## {title}\n").expect("a heading");
        if i == 12 {
            doc.push_str("The end of the guide.\n");
            break;
        }
        for j in 1..=4 {
            writeln!(doc, "{}", para(&format!("section {i}"), j)).expect("a line");
            if i == 2 && j == 2 {
                doc.push_str("### Section 2 details\n\n");
            }
        }
    }
    doc
}

/// Section 7's heading: longer than one line of the outline.
const OUTLINE_LONG: &str =
    "Section 7: a heading long enough that it cannot fit on one line of the outline";

/// M11ai Task 8's "On this page" (wide), on `outline_fixture()` (GUIDE.md), in both themes:
/// - `file-tab-md-reader-outline` (1,200 × 700): `outline_shown() == Some(true)`; the outline's
///   entries are `OUTLINE_ROW` high inside the `OUTLINE_W` column (`TERMINAL_PADDING` from the
///   right); a paragraph is centred in the reader's area, which ends `OUTLINE_GAP` left of the
///   column. At the top no entry is current. Wheel steps down: after each, the current entry is
///   the one the rule gives (`md_reader::current_entry`: the last heading at or above the top
///   quarter of the view) until section 5's heading is reached; then it is section 5's, its
///   `OUTLINE_BAR` drawn in the outline's bar colour (the entry above has none). A wheel step
///   that crosses no heading leaves it. Section 7's entry's tooltip is its full text. Scrolled
///   to the end (a wheel), the current entry is section 12's (in view, not reached: the end's
///   rule). A click on section 8's entry: its heading's row at the top `jump_offset` in, the
///   entry current, the scrollbar shown, the hovered entry on the panel hover fill. Shot then.
/// - `file-tab-md-reader-outline-narrow` (700 × 700): `Some(false)`: no column drawn, the
///   paragraph centred in the whole area.
/// - `file-tab-md-reader-outline-none` (1,200 × 700): a title and one section: `None`, and the
///   paragraph centred in the whole area.
fn file_tab_md_reader_outline_scenes() {
    use crate::agent_markdown::Md;
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use crate::md_reader::{current_entry, jump_offset};
    use crate::ui_tokens::reader::{
        HEADING_ABOVE, OUTLINE_BAR, OUTLINE_GAP, OUTLINE_ROW, OUTLINE_W, REACH,
    };
    use crate::ui_tokens::size::TERMINAL_PADDING;
    use gpui::{ListState, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-md-reader-outline");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let guide = dir.join("GUIDE.md");
    std::fs::write(&guide, outline_fixture()).expect("GUIDE.md");
    let one = dir.join("ONE.md");
    std::fs::write(
        &one,
        "# One section\n\nA short note with a single section under its title.\n\n\
         ## The only section\n\nThere is nothing else to read here.\n",
    )
    .expect("ONE.md");

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    // The rule, from the list as it is: reached (above the top row, or its top in the view's
    // top quarter), in view (its top over the view's bottom), at the end.
    let reach_line = |list: &ListState| {
        let v = list.viewport_bounds();
        f32::from(v.top()) + REACH * f32::from(v.size.height)
    };
    let reached = move |list: &ListState, row: usize| {
        row < list.logical_scroll_top().item_ix
            || list
                .bounds_for_item(row)
                .is_some_and(|b| f32::from(b.top()) <= reach_line(list))
    };
    let expected = move |list: &ListState, rows: &[usize]| {
        let bottom = list.viewport_bounds().bottom();
        current_entry(
            rows,
            list.is_scrolled_to_end() == Some(true),
            |r| reached(list, r),
            |r| list.bounds_for_item(r).is_some_and(|b| b.top() < bottom),
        )
    };
    let (side, _) = TERMINAL_PADDING;
    // A plain paragraph drawn now, centred in `left..right` (±1 pt).
    let centred = |cx: &mut VisualTestAppContext,
                   tab: &gpui::Entity<FileTab>,
                   left: f32,
                   right: f32,
                   n: &str| {
        let doc = cx
            .read(|cx| tab.read(cx).reader_doc())
            .expect("the reader's document");
        let drawn = cx.read(|cx| tab.read(cx).reader_drawn());
        let (row, b) = drawn
            .iter()
            .find(|(row, _)| {
                matches!(
                    doc.plan.row_kind(**row),
                    crate::md_reader::RowKind::Block(i) if matches!(doc.blocks[i], Md::Para(_))
                )
            })
            .unwrap_or_else(|| panic!("{n}: a paragraph is drawn"));
        let mid = (f32::from(b.left()) + f32::from(b.right())) / 2.0;
        println!("ui-shots: {n}: paragraph row {row} at {b:?}, area {left}..{right}");
        assert!(
            (mid - (left + right) / 2.0).abs() <= 1.0,
            "{n}: the paragraph (row {row}, {b:?}) is not centred in {left}..{right}"
        );
    };

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let colours = crate::md_preview::Colours::of(&theme);
        let build_for = |slot: TabSlot, path: std::path::PathBuf| {
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

        // 1,200 pt wide: the column.
        let n = format!("file-tab-md-reader-outline-{slug}");
        let dims = size(px(1200.0), px(700.0));
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_shown()),
                    Some(true),
                    "{n}: the column shows"
                );
                let rows = reader_entry_rows(cx, &tab);
                assert_eq!(rows.len(), 13, "{n}: 12 sections and a subsection");
                let list = cx.read(|cx| tab.read(cx).preview_list());
                let view = list.viewport_bounds();
                let (l, r) = (f32::from(view.left()), f32::from(view.right()));
                let column = (r - side - OUTLINE_W, r - side);
                // The entries: a row each, inside the column.
                for e in 0..rows.len() {
                    let b = cx
                        .read(|cx| tab.read(cx).outline_entry_bounds(e))
                        .unwrap_or_else(|| panic!("{n}: entry {e} is drawn"));
                    assert!(
                        f32::from(b.left()) >= column.0 - 0.5
                            && f32::from(b.right()) <= column.1 + 0.5
                            && (f32::from(b.size.height) - OUTLINE_ROW).abs() <= 0.5,
                        "{n}: entry {e} at {b:?}, not an {OUTLINE_ROW} row in {column:?}"
                    );
                }
                // The reader's area ends the gap left of the column.
                centred(cx, &tab, l + side, column.0 - OUTLINE_GAP, &n);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_current()),
                    None,
                    "{n}: at the top, no entry is current"
                );

                // Wheel steps down to section 5: the rule's entry after each.
                let at = point(view.left() + px(120.0), view.center().y);
                let five = reader_entry_of(cx, &tab, "Section 5");
                let mut steps = 0;
                while !reached(&list, rows[five]) {
                    reader_wheel(cx, window, at, 3.0);
                    steps += 1;
                    assert!(steps < 200, "{n}: section 5 is never reached");
                    assert_eq!(
                        cx.read(|cx| tab.read(cx).outline_current()),
                        expected(&list, &rows),
                        "{n}: after wheel step {steps}"
                    );
                }
                println!("ui-shots: {n}: section 5 reached after {steps} wheel steps");
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_current()),
                    Some(five),
                    "{n}: section 5 is current"
                );
                let shot = frame_now(cx, window);
                let b = cx
                    .read(|cx| tab.read(cx).outline_entry_bounds(five))
                    .expect("section 5's entry");
                let x = f32::from(b.left()) + OUTLINE_BAR / 2.0;
                let bar = pixel_at(&shot, dims, x, f32::from(b.center().y));
                assert!(
                    gap(bar, colours.outline_bar) <= 6,
                    "{n}: section 5's bar {bar:?} is not {:?}",
                    colours.outline_bar
                );
                let above = cx
                    .read(|cx| tab.read(cx).outline_entry_bounds(five - 1))
                    .expect("section 4's entry");
                let none = pixel_at(&shot, dims, x, f32::from(above.center().y));
                assert!(
                    gap(none, theme.background) <= 6,
                    "{n}: section 4's entry has no bar ({none:?})"
                );

                // A step that crosses no heading.
                let six = list
                    .bounds_for_item(rows[five + 1])
                    .map(|b| f32::from(b.top()))
                    .expect("section 6 is measured");
                assert!(
                    six - 60.0 > reach_line(&list) + 1.0,
                    "{n}: section 6 ({six}) is near the line"
                );
                let place = |list: &ListState| {
                    let top = list.logical_scroll_top();
                    (top.item_ix, f32::from(top.offset_in_item))
                };
                let before = place(&list);
                reader_wheel(cx, window, at, 3.0);
                assert_ne!(place(&list), before, "{n}: the wheel step scrolled");
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_current()),
                    Some(five),
                    "{n}: still section 5"
                );

                // Section 7's entry's tooltip: its full text.
                let seven = reader_entry_of(cx, &tab, OUTLINE_LONG);
                let b = cx
                    .read(|cx| tab.read(cx).outline_entry_bounds(seven))
                    .expect("section 7's entry");
                if let Ok(mut shown) = crate::sidebar_toggle::SHOWN_TIP.lock() {
                    *shown = None;
                }
                cx.simulate_mouse_move(window, b.center(), None, Modifiers::none());
                cx.advance_clock(Duration::from_secs(1));
                reader_settle(cx, window);
                draw(cx, window);
                let tip = crate::sidebar_toggle::SHOWN_TIP
                    .lock()
                    .ok()
                    .and_then(|t| t.clone());
                assert_eq!(tip.as_deref(), Some(OUTLINE_LONG), "{n}: the tooltip");
                cx.simulate_mouse_move(window, at, None, Modifiers::none());
                reader_settle(cx, window);

                // The end: the last heading in view, though not reached.
                reader_wheel(cx, window, at, 1000.0);
                assert_eq!(list.is_scrolled_to_end(), Some(true), "{n}: at the end");
                let last = rows.len() - 1;
                assert!(
                    !reached(&list, rows[last]),
                    "{n}: section 12 is under the top quarter"
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_current()),
                    Some(last),
                    "{n}: at the end, section 12 is current"
                );

                // A click on section 8's entry: a jump there.
                let eight = reader_entry_of(cx, &tab, "Section 8");
                let b = cx
                    .read(|cx| tab.read(cx).outline_entry_bounds(eight))
                    .expect("section 8's entry");
                cx.simulate_mouse_move(window, b.center(), None, Modifiers::none());
                cx.simulate_click(window, b.center(), Modifiers::none());
                reader_settle(cx, window);
                let top = list.logical_scroll_top();
                assert_eq!(
                    (top.item_ix, f32::from(top.offset_in_item)),
                    (rows[eight], jump_offset(HEADING_ABOVE)),
                    "{n}: section 8's heading at the top"
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_current()),
                    Some(eight),
                    "{n}: section 8 is current"
                );
                assert!(
                    cx.read(|cx| tab.read(cx).scrollbar_shown()),
                    "{n}: a jump shows the scrollbar"
                );
                let shot = frame_now(cx, window);
                let fill = pixel_at(
                    &shot,
                    dims,
                    f32::from(b.right()) - 4.0,
                    f32::from(b.center().y),
                );
                assert!(
                    gap(fill, colours.outline_hover) <= 6,
                    "{n}: the hovered entry's fill {fill:?} is not {:?}",
                    colours.outline_hover
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-outline",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), guide.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        // 700 pt wide: no column (the narrow button is Task 9's).
        let n = format!("file-tab-md-reader-outline-narrow-{slug}");
        let dims = size(px(700.0), px(700.0));
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_shown()),
                    Some(false),
                    "{n}: too narrow for the column"
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_entry_bounds(0)),
                    None,
                    "{n}: no column drawn"
                );
                let view = cx.read(|cx| tab.read(cx).preview_list()).viewport_bounds();
                let (l, r) = (f32::from(view.left()), f32::from(view.right()));
                centred(cx, &tab, l + side, r - side, &n);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-outline-narrow",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), guide.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        // One section under the title: no outline.
        let n = format!("file-tab-md-reader-outline-none-{slug}");
        let dims = size(px(1200.0), px(700.0));
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_shown()),
                    None,
                    "{n}: one entry: no outline"
                );
                let view = cx.read(|cx| tab.read(cx).preview_list()).viewport_bounds();
                let (l, r) = (f32::from(view.left()), f32::from(view.right()));
                centred(cx, &tab, l + side, r - side, &n);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-outline-none",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), one.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// A frame of the reader scenes (M11ai Task 9): a refresh, the effects settled.
fn reader_frame(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, _| window.refresh())
        .expect("refresh");
    cx.run_until_parked();
}

/// A frame, then each next frame something asked for (the current entry is worked out at the
/// start of the frame after a scroll; a fit change draws the tab again), each drawn.
fn reader_settle(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    reader_frame(cx, window);
    for _ in 0..10 {
        if !next_frame(cx, window) {
            break;
        }
        reader_frame(cx, window);
    }
}

/// The file tab's Preview turned on with its switch.
fn reader_preview_on(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &gpui::Entity<crate::file_tab::FileTab>,
) {
    reader_settle(cx, window);
    let button = cx
        .read(|cx| tab.read(cx).preview_bounds())
        .expect("Preview is drawn");
    cx.simulate_click(window, button.center(), gpui::Modifiers::none());
    reader_settle(cx, window);
    reader_settle(cx, window);
}

/// A wheel of `lines` down at `at`, then settled.
fn reader_wheel(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    at: gpui::Point<Pixels>,
    lines: f32,
) {
    use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase};
    cx.simulate_event(
        window,
        ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Lines(point(0.0, -lines)),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        },
    );
    reader_settle(cx, window);
}

/// The "On this page" entry whose heading reads `text`.
fn reader_entry_of(
    cx: &mut VisualTestAppContext,
    tab: &gpui::Entity<crate::file_tab::FileTab>,
    text: &str,
) -> usize {
    let doc = cx
        .read(|cx| tab.read(cx).reader_doc())
        .expect("the reader's document");
    doc.plan
        .outline
        .iter()
        .position(|e| doc.plan.headings[e.heading].text == text)
        .unwrap_or_else(|| panic!("the entry {text}"))
}

/// Each "On this page" entry's heading's list row.
fn reader_entry_rows(
    cx: &mut VisualTestAppContext,
    tab: &gpui::Entity<crate::file_tab::FileTab>,
) -> Vec<usize> {
    let doc = cx
        .read(|cx| tab.read(cx).reader_doc())
        .expect("the reader's document");
    doc.plan
        .outline
        .iter()
        .map(|e| doc.plan.row_of_block(doc.plan.headings[e.heading].block))
        .collect()
}

/// The fixtures of Task 9's scenes: GUIDE.md (`outline_fixture()`) and ONE.md (fits a 600 pt
/// Preview).
fn reader_task_9_fixtures(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(name);
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let guide = dir.join("GUIDE.md");
    std::fs::write(&guide, outline_fixture()).expect("GUIDE.md");
    let one = dir.join("ONE.md");
    std::fs::write(
        &one,
        "# One section\n\nA short note with a single section under its title.\n\n\
         ## The only section\n\nThere is nothing else to read here.\n",
    )
    .expect("ONE.md");
    (guide, one)
}

/// M11ai Task 9's narrow "On this page", on `outline_fixture()` (GUIDE.md), in both themes:
/// - `file-tab-md-reader-narrow` (700 × 600): `outline_shown() == Some(false)`; the `Outline`
///   button drawn at the Preview's top-right (`TERMINAL_PADDING` in), the panel closed. A click
///   opens the panel under it (its entries below the button, their right edge by the button's);
///   a click on section 8's entry: its heading at the top `jump_offset` in, the entry current,
///   the panel closed. Open again, Esc closes it (a second Esc leaves Preview on); open again, a
///   press on the reader closes it; a click on the button while open closes it; ⌘F, Preview off
///   and a mode switch (each turned back) each close it.
/// - `file-tab-md-reader-narrow-panel` (700 × 600): wheel steps until section 2 is current,
///   then the panel opened: the current entry the same, its bar in the panel's bar colour (the
///   entry above has none). Shot then.
/// - `file-tab-md-reader-narrow-wide` (1,200 × 600, the tab 700 wide in a host): the panel
///   open, the tab widened to 1,200: the column shows and the panel is closed (and no button);
///   narrowed back: the button again, the panel still closed.
fn file_tab_md_reader_narrow_scenes() {
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use crate::md_reader::jump_offset;
    use crate::ui_tokens::reader::{HEADING_ABOVE, OUTLINE_BAR, OUTLINE_W};
    use crate::ui_tokens::size::TERMINAL_PADDING;
    use gpui::{Entity, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// The tab in a pane `width` wide (the window stays as it is).
    struct WideHost {
        tab: Entity<FileTab>,
        width: Pixels,
    }
    impl Render for WideHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_row()
                .child(div().h_full().w(self.width).flex().child(self.tab.clone()))
        }
    }

    let (guide, _) = reader_task_9_fixtures("fuxx-ui-shots-md-reader-narrow");
    type TabSlot = Rc<RefCell<Option<Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    let (side, top_pad) = TERMINAL_PADDING;
    let click = |cx: &mut VisualTestAppContext, window, at: gpui::Point<Pixels>| {
        cx.simulate_mouse_move(window, at, None, Modifiers::none());
        cx.simulate_click(window, at, Modifiers::none());
        reader_settle(cx, window);
    };
    let open = move |cx: &mut VisualTestAppContext, window, tab: &Entity<FileTab>, n: &str| {
        let b = cx
            .read(|cx| tab.read(cx).outline_button_bounds())
            .unwrap_or_else(|| panic!("{n}: the button is drawn"));
        click(cx, window, b.center());
        assert!(
            cx.read(|cx| tab.read(cx).outline_panel_open()),
            "{n}: the button opens the panel"
        );
        b
    };
    let is_open = |cx: &mut VisualTestAppContext, tab: &Entity<FileTab>| {
        cx.read(|cx| tab.read(cx).outline_panel_open())
    };

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let colours = crate::md_preview::Colours::of(&theme);
        let build_for = |slot: TabSlot, path: std::path::PathBuf| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(path.clone(), FileMode::Plain, window, cx);
                    tab.frozen = true;
                    tab
                });
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };

        // The button, the panel and what closes it.
        let n = format!("file-tab-md-reader-narrow-{slug}");
        let dims = size(px(700.0), px(600.0));
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_shown()),
                    Some(false),
                    "{n}: too narrow for the column"
                );
                let view = cx.read(|cx| tab.read(cx).preview_list()).viewport_bounds();
                let b = cx
                    .read(|cx| tab.read(cx).outline_button_bounds())
                    .unwrap_or_else(|| panic!("{n}: the button is drawn"));
                println!("ui-shots: {n}: the button at {b:?}, the Preview at {view:?}");
                assert!(
                    (f32::from(b.right()) - (f32::from(view.right()) - side)).abs() <= 0.5
                        && (f32::from(b.top()) - (f32::from(view.top()) + top_pad)).abs() <= 0.5,
                    "{n}: the button {b:?} is not at the Preview's top-right ({view:?})"
                );
                // Its glyph: ink in its box.
                let shot = frame_now(cx, window);
                let inked = (0..24).any(|i| {
                    (0..24).any(|j| {
                        let p = pixel_at(
                            &shot,
                            dims,
                            f32::from(b.left()) + i as f32 * f32::from(b.size.width) / 24.0,
                            f32::from(b.top()) + j as f32 * f32::from(b.size.height) / 24.0,
                        );
                        gap(p, theme.background) > 60
                    })
                });
                assert!(inked, "{n}: the button's glyph is drawn");
                assert!(!is_open(cx, &tab), "{n}: the panel starts closed");

                // Open: the entries under the button, as wide as the panel lets them be.
                let b = open(cx, window, &tab, &n);
                for e in 0..5 {
                    let r = cx
                        .read(|cx| tab.read(cx).outline_entry_bounds(e))
                        .unwrap_or_else(|| panic!("{n}: panel entry {e} is drawn"));
                    assert!(
                        r.top() >= b.bottom()
                            && f32::from(r.right()) <= f32::from(b.right()) + 0.5
                            && f32::from(r.left()) >= f32::from(b.right()) - OUTLINE_W - 0.5,
                        "{n}: panel entry {e} at {r:?}, not under the button {b:?}"
                    );
                }

                // An entry's click jumps there and closes the panel.
                let eight = reader_entry_of(cx, &tab, "Section 8");
                let r = cx
                    .read(|cx| tab.read(cx).outline_entry_bounds(eight))
                    .unwrap_or_else(|| panic!("{n}: section 8's entry is drawn"));
                click(cx, window, r.center());
                let list = cx.read(|cx| tab.read(cx).preview_list());
                let top = list.logical_scroll_top();
                assert_eq!(
                    (top.item_ix, f32::from(top.offset_in_item)),
                    (
                        reader_entry_rows(cx, &tab)[eight],
                        jump_offset(HEADING_ABOVE)
                    ),
                    "{n}: section 8's heading at the top"
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_current()),
                    Some(eight),
                    "{n}: section 8 is current"
                );
                assert!(!is_open(cx, &tab), "{n}: an entry's click closes the panel");

                // Esc closes it; with it closed, Esc leaves the Preview as it is.
                open(cx, window, &tab, &n);
                cx.simulate_keystrokes(window, "escape");
                reader_settle(cx, window);
                assert!(!is_open(cx, &tab), "{n}: Esc closes the panel");
                cx.simulate_keystrokes(window, "escape");
                reader_settle(cx, window);
                assert!(
                    cx.read(|cx| tab.read(cx).preview_shown()),
                    "{n}: a second Esc leaves Preview on"
                );

                // A press on the reader closes it.
                open(cx, window, &tab, &n);
                let reader = point(view.left() + px(120.0), view.center().y);
                click(cx, window, reader);
                assert!(!is_open(cx, &tab), "{n}: a press outside closes the panel");

                // The button again closes it.
                let b = open(cx, window, &tab, &n);
                click(cx, window, b.center());
                assert!(!is_open(cx, &tab), "{n}: the button closes the open panel");

                // ⌘F (Preview off for the find bar), then Preview on again: closed.
                open(cx, window, &tab, &n);
                cx.simulate_keystrokes(window, "cmd-f");
                reader_settle(cx, window);
                assert!(
                    !cx.read(|cx| tab.read(cx).preview_shown()),
                    "{n}: ⌘F turns Preview off"
                );
                let preview = |cx: &mut VisualTestAppContext, on: bool| {
                    cx.update_window(window, |_, window, cx| {
                        tab.update(cx, |t, cx| t.choose_preview_for_shots(on, window, cx))
                    })
                    .expect("the switch");
                    reader_settle(cx, window);
                    reader_settle(cx, window);
                };
                preview(cx, true);
                assert!(!is_open(cx, &tab), "{n}: ⌘F closed the panel");

                // Preview off and on: closed.
                open(cx, window, &tab, &n);
                preview(cx, false);
                preview(cx, true);
                assert!(!is_open(cx, &tab), "{n}: Preview off closed the panel");

                // A mode switch and back: closed.
                open(cx, window, &tab, &n);
                for mode in [FileMode::Diff, FileMode::Plain] {
                    cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(mode, cx)));
                    reader_settle(cx, window);
                }
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_shown()),
                    Some(false),
                    "{n}: back in Preview"
                );
                assert!(!is_open(cx, &tab), "{n}: a mode switch closed the panel");
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-narrow",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), guide.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        // The panel open, with a current entry (the shot).
        let n = format!("file-tab-md-reader-narrow-panel-{slug}");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let view = cx.read(|cx| tab.read(cx).preview_list()).viewport_bounds();
                let at = point(view.left() + px(120.0), view.center().y);
                let two = reader_entry_of(cx, &tab, "Section 2");
                let mut steps = 0;
                while cx.read(|cx| tab.read(cx).outline_current()) != Some(two) {
                    reader_wheel(cx, window, at, 3.0);
                    steps += 1;
                    assert!(steps < 100, "{n}: section 2 is never current");
                }
                open(cx, window, &tab, &n);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_current()),
                    Some(two),
                    "{n}: the same current entry"
                );
                // Away from the entries, so none is hovered in the shot.
                cx.simulate_mouse_move(
                    window,
                    point(view.left() + px(40.0), view.bottom() - px(20.0)),
                    None,
                    Modifiers::none(),
                );
                reader_settle(cx, window);
                let shot = frame_now(cx, window);
                let b = cx
                    .read(|cx| tab.read(cx).outline_entry_bounds(two))
                    .expect("section 2's entry");
                let x = f32::from(b.left()) + OUTLINE_BAR / 2.0;
                let bar = pixel_at(&shot, dims, x, f32::from(b.center().y));
                assert!(
                    gap(bar, colours.outline_panel.bar) <= 6,
                    "{n}: section 2's bar {bar:?} is not {:?}",
                    colours.outline_panel.bar
                );
                let above = cx
                    .read(|cx| tab.read(cx).outline_entry_bounds(two - 1))
                    .expect("section 1's entry");
                let none = pixel_at(&shot, dims, x, f32::from(above.center().y));
                assert!(
                    gap(none, theme.ui().panel) <= 6,
                    "{n}: section 1's entry has no bar ({none:?})"
                );
                assert!(is_open(cx, &tab), "{n}: open for the shot");
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-narrow-panel",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), guide.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        // Wide again: the panel closes.
        let n = format!("file-tab-md-reader-narrow-wide-{slug}");
        let dims = size(px(1200.0), px(600.0));
        type HostSlot = Rc<RefCell<Option<(Entity<FileTab>, Entity<WideHost>)>>>;
        let slot: HostSlot = Rc::default();
        let build = {
            let (slot, path) = (slot.clone(), guide.clone());
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(path, FileMode::Plain, window, cx);
                    tab.frozen = true;
                    tab
                });
                let host = cx.new(|_| WideHost {
                    tab: tab.clone(),
                    width: px(700.0),
                });
                *slot.borrow_mut() = Some((tab, host.clone()));
                host.into()
            }
        };
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let (tab, host) = slot.borrow().clone().expect("the tab");
                reader_preview_on(cx, window, &tab);
                let width = |cx: &mut VisualTestAppContext, w: f32| {
                    cx.update(|cx| {
                        host.update(cx, |h, cx| {
                            h.width = px(w);
                            cx.notify();
                        })
                    });
                    reader_settle(cx, window);
                    reader_settle(cx, window);
                };
                open(cx, window, &tab, &n);
                width(cx, 1200.0);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_shown()),
                    Some(true),
                    "{n}: wide: the column"
                );
                assert!(!is_open(cx, &tab), "{n}: wide closes the panel");
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_button_bounds()),
                    None,
                    "{n}: wide: no button"
                );
                width(cx, 700.0);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_shown()),
                    Some(false),
                    "{n}: narrow again"
                );
                assert!(
                    cx.read(|cx| tab.read(cx).outline_button_bounds()).is_some(),
                    "{n}: narrow again: the button"
                );
                assert!(!is_open(cx, &tab), "{n}: narrow again: still closed");
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-narrow-wide",
            slug,
            &theme,
            dims,
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// M11ai Task 9's reading-progress line, in both themes (700 × 600):
/// - `file-tab-md-reader-progress` on `outline_fixture()` (GUIDE.md): at the top
///   `progress_share() == Some(0)` and no accent along the Preview's top edge past `PROGRESS`
///   pt in. Wheel steps to about half-way: the share within 0.05 of the list's own (its offset
///   over the room to scroll); along the top edge the line's colour up to that share of the
///   width, and none past it. At the end (a wheel) the share is 1 and the line runs all along.
/// - `file-tab-md-reader-progress-fits` on ONE.md (it fits): `None`, and no line.
fn file_tab_md_reader_progress_scenes() {
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use crate::ui_tokens::reader::PROGRESS;
    use gpui::{Entity, ListState};
    use std::cell::RefCell;
    use std::rc::Rc;

    let (guide, one) = reader_task_9_fixtures("fuxx-ui-shots-md-reader-progress");
    type TabSlot = Rc<RefCell<Option<Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    // The list's own share scrolled: its offset over the room to scroll.
    let list_share = |list: &ListState| {
        let height = f32::from(list.viewport_bounds().size.height);
        let content = f32::from(list.max_offset_for_scrollbar().y) + height;
        let offset = -f32::from(list.scroll_px_offset_for_scrollbar().y);
        offset / (content - height)
    };
    // Clear of the scrollbar's zone at the Preview's right edge.
    const CLEAR: f32 = 16.0;

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let line = crate::md_preview::Colours::of(&theme).progress;
        let dims = size(px(700.0), px(600.0));
        let build_for = |slot: TabSlot, path: std::path::PathBuf| {
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
        // The pixels along the Preview's top edge (in its `PROGRESS` pt), from `from` to `to`
        // every 6 pt: each the line's colour (`on`) or not.
        let along = move |cx: &mut VisualTestAppContext,
                          window: AnyWindowHandle,
                          view: gpui::Bounds<Pixels>,
                          (from, to): (f32, f32),
                          on: bool,
                          n: &str| {
            let shot = frame_now(cx, window);
            let y = f32::from(view.top()) + PROGRESS / 2.0;
            let mut x = from;
            while x <= to {
                let p = pixel_at(&shot, dims, x, y);
                if on {
                    assert!(gap(p, line) <= 6, "{n}: at x {x}: {p:?} is not the line");
                } else {
                    assert!(gap(p, line) > 24, "{n}: at x {x}: {p:?} is the line");
                }
                x += 6.0;
            }
        };

        let n = format!("file-tab-md-reader-progress-{slug}");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let list = cx.read(|cx| tab.read(cx).preview_list());
                let view = list.viewport_bounds();
                let (l, r) = (f32::from(view.left()), f32::from(view.right()));
                let share = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| tab.read(cx).progress_share())
                        .unwrap_or_else(|| panic!("{n}: the line shows"))
                };
                // The top.
                assert!(share(cx).abs() <= 0.001, "{n}: at the top, {}", share(cx));
                along(cx, window, view, (l + PROGRESS + 1.0, r - CLEAR), false, &n);

                // About half-way.
                let at = point(view.left() + px(120.0), view.center().y);
                let mut steps = 0;
                while list_share(&list) < 0.45 {
                    reader_wheel(cx, window, at, 3.0);
                    steps += 1;
                    assert!(steps < 300, "{n}: never half-way");
                }
                let (got, want) = (share(cx), list_share(&list));
                println!("ui-shots: {n}: half-way after {steps} steps: {got} (the list's {want})");
                assert!((got - want).abs() <= 0.05, "{n}: {got}, the list's {want}");
                let end = l + got * (r - l);
                along(cx, window, view, (l + 1.0, end - 3.0), true, &n);
                along(cx, window, view, (end + 3.0, r - CLEAR), false, &n);

                // The end.
                reader_wheel(cx, window, at, 1000.0);
                assert_eq!(list.is_scrolled_to_end(), Some(true), "{n}: at the end");
                assert!(share(cx) >= 0.999, "{n}: at the end, {}", share(cx));
                along(cx, window, view, (l + 1.0, r - 3.0), true, &n);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-progress",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), guide.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());

        let n = format!("file-tab-md-reader-progress-fits-{slug}");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                assert!(
                    cx.read(|cx| tab.read(cx).preview_shown()),
                    "{n}: Preview shows"
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).progress_share()),
                    None,
                    "{n}: the doc fits: no line"
                );
                let view = cx.read(|cx| tab.read(cx).preview_list()).viewport_bounds();
                let (l, r) = (f32::from(view.left()), f32::from(view.right()));
                along(cx, window, view, (l + 1.0, r - CLEAR), false, &n);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-progress-fits",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), one.clone()),
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// Task 10's reload fixture: `outline_fixture()` with a `rust` block at the top of section 1 and
/// an `sh` block at the top of section 6, after front matter when `front` is true.
fn reload_fixture(front: bool) -> String {
    let body = outline_fixture()
        .replacen(
            "## Section 1\n\n",
            "## Section 1\n\n```rust\nfn main() {\n    my_app::serve();\n}\n```\n\n",
            1,
        )
        .replacen(
            "## Section 6\n\n",
            "## Section 6\n\n```sh\n./install.sh --prefix ~/.local\n```\n\n",
            1,
        );
    if front {
        format!("---\ntitle: Field guide\nowner: docs-team\n---\n\n{body}")
    } else {
        body
    }
}

/// M11ai Task 10's live reload (Review Focus 4), on `reload_fixture` (GUIDE.md), one theme:
/// - `file-tab-md-reader-reload` (700 × 600, the outline narrow): section 6's heading at the
///   top, Copy pressed on its `sh` block ("Copied"), the narrow panel open. The file rewritten
///   with front matter at the top: in the first frame after it is read, the same heading at the
///   top (its row + 1, the offset kept), no "Copied", both code blocks coloured from the cache
///   (no new colour job); settled, the panel still open with the new entries (section 6's at
///   its new row) and section 6 current. The front matter removed: the row − 1 again. The file
///   cut to a single section (no outline): the panel closes; the guide back: it stays closed
///   (not reopened unasked). The other theme (light ↔ dark): the code colours worked out again
///   for it (other runs' colours than before).
/// - `file-tab-md-reader-reload-syntaxes`: Preview on while the syntaxes are still loading
///   (held), off, the syntaxes in, on again: the code blocks are coloured (the Task 4 race).
fn file_tab_md_reader_reload_scenes() {
    use super::branch::settle_until;
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use crate::md_preview::Copied;
    use gpui::{Entity, ListOffset, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-md-reader-reload");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    type TabSlot = Rc<RefCell<Option<Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    // The file rewritten, the tab told to look again, and one frame drawn (the new version is
    // read in it).
    let rewrite = |cx: &mut VisualTestAppContext,
                   window: AnyWindowHandle,
                   tab: &Entity<FileTab>,
                   path: &std::path::Path,
                   text: &str,
                   n: &str| {
        let before = cx.read(|cx| tab.read(cx).parsed_version());
        std::fs::write(path, text).expect("rewrite");
        cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
        cx.run_until_parked();
        reader_frame(cx, window);
        let after = cx.read(|cx| tab.read(cx).parsed_version());
        assert!(after > before, "{n}: read again ({before:?} → {after:?})");
    };
    let coloured = |cx: &mut VisualTestAppContext, tab: &Entity<FileTab>| {
        cx.read(|cx| tab.read(cx).code_coloured())
    };

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let dims = size(px(700.0), px(600.0));
        let build_for = |slot: TabSlot, path: std::path::PathBuf, hold: bool| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                if hold {
                    FileTab::hold_syntaxes_for_shots(cx);
                }
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(path.clone(), FileMode::Plain, window, cx);
                    tab.frozen = true;
                    tab
                });
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };

        let n = format!("file-tab-md-reader-reload-{slug}");
        let guide = dir.join("GUIDE.md");
        std::fs::write(&guide, reload_fixture(false)).expect("GUIDE.md");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot, path) = (n.clone(), slot.clone(), guide.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let done = settle_until(cx, window, 20, |cx| coloured(cx, &tab) == 2);
                assert!(
                    done,
                    "{n}: both code blocks coloured, not {}",
                    coloured(cx, &tab)
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_shown()),
                    Some(false),
                    "{n}: the outline is narrow"
                );
                let list = cx.read(|cx| tab.read(cx).preview_list());
                let six = reader_entry_of(cx, &tab, "Section 6");
                let row = reader_entry_rows(cx, &tab)[six];
                list.scroll_to(ListOffset {
                    item_ix: row,
                    offset_in_item: px(0.0),
                });
                reader_settle(cx, window);

                // Copy on section 6's `sh` block (the row under its heading), then the panel.
                let sh = row + 1;
                let copy = cx
                    .read(|cx| tab.read(cx).copy_bounds(sh))
                    .unwrap_or_else(|| panic!("{n}: the sh block's Copy is drawn"));
                cx.simulate_click(window, copy.center(), Modifiers::none());
                reader_frame(cx, window);
                let b = cx
                    .read(|cx| tab.read(cx).outline_button_bounds())
                    .unwrap_or_else(|| panic!("{n}: the outline's button is drawn"));
                cx.simulate_mouse_move(window, b.center(), None, Modifiers::none());
                cx.simulate_click(window, b.center(), Modifiers::none());
                reader_frame(cx, window);
                assert!(
                    cx.read(|cx| tab.read(cx).outline_panel_open()),
                    "{n}: the panel is open"
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).copied()),
                    Some(Copied::Code(sh)),
                    "{n}: Copied shows before the reload"
                );
                let looks = cx.read(|cx| tab.read(cx).code_looks_for_shots());
                let jobs = cx.read(|cx| tab.read(cx).code_jobs());

                // Front matter added at the top.
                rewrite(cx, window, &tab, &path, &reload_fixture(true), &n);
                // A new version has a new list.
                let top = cx
                    .read(|cx| tab.read(cx).preview_list())
                    .logical_scroll_top();
                assert_eq!(
                    (top.item_ix, f32::from(top.offset_in_item)),
                    (row + 1, 0.0),
                    "{n}: front matter added: section 6's heading at the top (row + 1)"
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).copied()),
                    None,
                    "{n}: Copied goes"
                );
                assert_eq!(coloured(cx, &tab), 2, "{n}: the colours kept at once");
                assert_eq!(
                    cx.read(|cx| tab.read(cx).code_jobs()),
                    jobs,
                    "{n}: the colours come from the cache (no job)"
                );
                reader_settle(cx, window);
                assert!(
                    cx.read(|cx| tab.read(cx).outline_panel_open()),
                    "{n}: the panel stays open"
                );
                assert_eq!(
                    reader_entry_rows(cx, &tab)[six],
                    row + 1,
                    "{n}: the panel's entries are the new version's"
                );
                assert!(
                    cx.read(|cx| tab.read(cx).outline_entry_bounds(six))
                        .is_some(),
                    "{n}: section 6's entry is drawn in the panel"
                );
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_current()),
                    Some(six),
                    "{n}: section 6 is current"
                );

                // The front matter removed.
                rewrite(cx, window, &tab, &path, &reload_fixture(false), &n);
                let top = cx
                    .read(|cx| tab.read(cx).preview_list())
                    .logical_scroll_top();
                assert_eq!(
                    (top.item_ix, f32::from(top.offset_in_item)),
                    (row, 0.0),
                    "{n}: front matter removed: section 6's heading at the top (row − 1)"
                );
                assert_eq!(coloured(cx, &tab), 2, "{n}: the colours kept at once");
                reader_settle(cx, window);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_current()),
                    Some(six),
                    "{n}: section 6 is still current"
                );
                assert!(
                    cx.read(|cx| tab.read(cx).outline_panel_open()),
                    "{n}: the panel stays open"
                );

                // No outline: the panel closes; the outline back: it stays closed.
                let one =
                    "# One section\n\nA short note.\n\n## The only section\n\nNothing else.\n";
                rewrite(cx, window, &tab, &path, one, &n);
                reader_settle(cx, window);
                assert!(
                    !cx.read(|cx| tab.read(cx).outline_panel_open()),
                    "{n}: no outline: the panel is closed"
                );
                rewrite(cx, window, &tab, &path, &reload_fixture(false), &n);
                reader_settle(cx, window);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).outline_shown()),
                    Some(false),
                    "{n}: the outline is back"
                );
                assert!(
                    !cx.read(|cx| tab.read(cx).outline_panel_open()),
                    "{n}: the panel does not reopen unasked"
                );

                // The other theme: the colours worked out again for it.
                let other = shot_theme(if slug == "mocha" {
                    "Catppuccin Latte"
                } else {
                    "Catppuccin Mocha"
                });
                cx.update(|cx| cx.set_global(shot_appearance(other, cx)));
                let done = settle_until(cx, window, 20, |cx| {
                    cx.read(|cx| tab.read(cx).code_theme()) == Some(other)
                        && coloured(cx, &tab) == 2
                });
                assert!(done, "{n}: the colours for the other theme");
                let now = cx.read(|cx| tab.read(cx).code_looks_for_shots());
                for (b, look) in &looks {
                    let then: Vec<_> = look.runs.iter().map(|(_, c)| *c).collect();
                    let again: Vec<_> = now[b].runs.iter().map(|(_, c)| *c).collect();
                    assert_ne!(
                        then, again,
                        "{n}: block {b}'s colours are the other theme's"
                    );
                }
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-reload",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), guide.clone(), false),
            act,
        );
        println!("ui-shots: {}", path.display());

        // The Task 4 race: Preview on while the syntaxes load, off, on again once they are in.
        let n = format!("file-tab-md-reader-reload-syntaxes-{slug}");
        let guide = dir.join("RACE.md");
        std::fs::write(&guide, reload_fixture(false)).expect("RACE.md");
        let slot: TabSlot = Rc::default();
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                assert_eq!(coloured(cx, &tab), 0, "{n}: the syntaxes are not in yet");
                let preview = |cx: &mut VisualTestAppContext, on: bool| {
                    cx.update_window(window, |_, window, cx| {
                        tab.update(cx, |t, cx| t.choose_preview_for_shots(on, window, cx))
                    })
                    .expect("the switch");
                    reader_settle(cx, window);
                };
                preview(cx, false);
                cx.update(FileTab::release_syntaxes_for_shots);
                reader_settle(cx, window);
                preview(cx, true);
                let done = settle_until(cx, window, 10, |cx| coloured(cx, &tab) == 2);
                assert!(
                    done,
                    "{n}: both code blocks coloured, not {}",
                    coloured(cx, &tab)
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-md-reader-reload-syntaxes",
            slug,
            &theme,
            dims,
            build_for(slot.clone(), guide.clone(), true),
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// Task 10's big document: a title, then 300 parts (`## Part N` and 9 one-line paragraphs):
/// 3,001 blocks, over `PREVIEW_MEASURE_ALL`.
fn big_fixture() -> String {
    use std::fmt::Write as _;
    let mut doc = String::from("# Big guide\n\n");
    for i in 0..300 {
        writeln!(doc, "## Part {i}\n").expect("a heading");
        for j in 1..=9 {
            writeln!(doc, "Line {j} of part {i}.\n").expect("a line");
        }
    }
    doc
}

/// M11ai Task 10's big document (`big_fixture`, BIG.md), one theme:
/// - `file-tab-md-reader-big` (1,200 × 700): Preview on, more rows than `PREVIEW_MEASURE_ALL`.
///   Five wheel events before a frame ask for one current-entry check at its start (not
///   five). Wheel steps down: after each, the current entry is the last part whose heading's
///   row is at or above the list's top block, or the next one when it is within the part
///   under it (reached in the view's top quarter); the entries only go forward; the progress
///   share grows. Scrolled to the end, the last part reached (by block index) is current and
///   the share is 1. Nothing panics.
fn file_tab_md_reader_big_scenes() {
    use crate::file_tab::{FileTab, PREVIEW_MEASURE_ALL};
    use crate::layout_model::FileMode;
    use gpui::{Entity, Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-md-reader-big");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let big = dir.join("BIG.md");
    std::fs::write(&big, big_fixture()).expect("BIG.md");
    type TabSlot = Rc<RefCell<Option<Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let dims = size(px(1200.0), px(700.0));
        let n = format!("file-tab-md-reader-big-{slug}");
        let slot: TabSlot = Rc::default();
        let build = {
            let (slot, path) = (slot.clone(), big.clone());
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(path, FileMode::Plain, window, cx);
                    tab.frozen = true;
                    tab
                });
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                reader_preview_on(cx, window, &tab);
                let list = cx.read(|cx| tab.read(cx).preview_list());
                assert!(
                    list.item_count() > PREVIEW_MEASURE_ALL,
                    "{n}: {} rows, over {PREVIEW_MEASURE_ALL}",
                    list.item_count()
                );
                let rows = reader_entry_rows(cx, &tab);
                assert_eq!(rows.len(), 300, "{n}: an entry per part");
                let view = list.viewport_bounds();
                let at = point(view.left() + px(120.0), view.center().y);

                // Five wheel events before the next frame: one check at its start.
                for _ in 0..5 {
                    cx.simulate_event(
                        window,
                        ScrollWheelEvent {
                            position: at,
                            delta: ScrollDelta::Lines(point(0.0, -1.0)),
                            modifiers: Modifiers::none(),
                            touch_phase: TouchPhase::Moved,
                        },
                    );
                }
                let ran = cx
                    .update_window(window, |_, window, cx| window.simulate_next_frame(cx))
                    .expect("the next frame");
                assert_eq!(ran, 1, "{n}: one current-entry check for five wheel events");
                reader_settle(cx, window);

                // Wheel steps: the current entry follows by block index.
                let mut last: Option<usize> = None;
                let mut share = cx.read(|cx| tab.read(cx).progress_share()).unwrap_or(0.0);
                let first = share;
                for step in 0..40 {
                    reader_wheel(cx, window, at, 12.0);
                    let top = list.logical_scroll_top().item_ix;
                    let reached = rows.iter().rposition(|r| *r <= top);
                    let next = reached.map_or(0, |e| e + 1);
                    let now = cx.read(|cx| tab.read(cx).outline_current());
                    let ok = now == reached
                        || (now == Some(next) && rows.get(next).is_some_and(|r| *r <= top + 10));
                    assert!(
                        ok,
                        "{n}: step {step}: top block {top}: current {now:?}, not {reached:?}"
                    );
                    assert!(now >= last, "{n}: step {step}: {now:?} after {last:?}");
                    last = now;
                    let s = cx.read(|cx| tab.read(cx).progress_share()).unwrap_or(0.0);
                    assert!(s >= share, "{n}: step {step}: the share {s} after {share}");
                    share = s;
                }
                println!(
                    "ui-shots: {n}: after 40 steps: top {:?}, current {last:?}, share {share}",
                    list.logical_scroll_top()
                );
                assert!(
                    last.is_some_and(|e| e >= 3),
                    "{n}: the entries moved on ({last:?})"
                );
                assert!(share > first, "{n}: the share moved ({first} → {share})");

                // The end.
                // A jump near the end (as a thumb drag lands), then wheel steps to it.
                list.scroll_to(gpui::ListOffset {
                    item_ix: list.item_count() - 40,
                    offset_in_item: px(0.0),
                });
                reader_settle(cx, window);
                for _ in 0..50 {
                    let place = |l: &gpui::ListState| {
                        let t = l.logical_scroll_top();
                        (t.item_ix, t.offset_in_item)
                    };
                    let was = place(&list);
                    reader_wheel(cx, window, at, 100.0);
                    if place(&list) == was {
                        break;
                    }
                }
                // (A list with unmeasured rows never says it is at the end: the last part
                // current is the one reached, by block index.)
                let top = list.logical_scroll_top().item_ix;
                assert!(
                    top + 20 >= list.item_count(),
                    "{n}: at the end (top block {top})"
                );
                let now = cx.read(|cx| tab.read(cx).outline_current());
                let reached = rows.iter().rposition(|r| *r <= top);
                println!("ui-shots: {n}: at the end: top block {top}, current {now:?}");
                assert!(
                    now >= reached && now >= Some(298),
                    "{n}: at the end: current {now:?}, reached {reached:?}"
                );
                let s = cx.read(|cx| tab.read(cx).progress_share()).unwrap_or(0.0);
                assert!(s >= 0.99, "{n}: at the end the share is {s}");
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with("file-tab-md-reader-big", slug, &theme, dims, build, act);
        println!("ui-shots: {}", path.display());
    }
}
