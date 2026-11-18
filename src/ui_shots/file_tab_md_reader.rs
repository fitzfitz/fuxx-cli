//! M11ai: the Markdown reader in the file tab's Preview: its centred column, type and spacing
//! (Task 3), on `READER_FIXTURE`, a made-up "Setup guide" the later tasks' scenes read too.

use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[Scene {
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
}];

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
///   table at least the column wide and inside the area; the long paragraph fills the column;
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
    use crate::ui_tokens::reader::{BODY, BODY_LINE, MEASURE};
    use crate::ui_tokens::size::TERMINAL_PADDING;
    use gpui::{Bounds, ListOffset, Modifiers, Pixels, SharedString};
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
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    let preview = move |cx: &mut VisualTestAppContext, window, tab: &gpui::Entity<FileTab>| {
        frame(cx, window);
        let button = cx
            .read(|cx| tab.read(cx).preview_bounds())
            .expect("Preview is drawn");
        cx.simulate_click(window, button.center(), Modifiers::none());
        frame(cx, window);
    };
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
            frame(cx, window);
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
        frame(cx, window);
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
                preview(cx, window, &tab);
                let doc = cx
                    .read(|cx| tab.read(cx).reader_doc())
                    .expect("the reader's document");
                let list = cx.read(|cx| tab.read(cx).preview_list());
                assert_eq!(doc.plan.front_rows(), 1, "{n}: the front matter's row");
                assert_eq!(list.item_count(), doc.plan.rows(), "{n}: a row each");
                let all = walk(cx, window, &tab);
                tall_enough(cx, window, &tab, &all, &n);
                let view = list.viewport_bounds();
                let (left, right) = (
                    f32::from(view.left()) + side,
                    f32::from(view.right()) - side,
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
                preview(cx, window, &tab);
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
                frame(cx, window);
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
                preview(cx, window, &tab);
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
