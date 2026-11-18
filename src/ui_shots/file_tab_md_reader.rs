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
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    // Preview on, then frames until the colour job's answer is in (the syntaxes load and the
    // blocks colour on the background executor).
    let preview = move |cx: &mut VisualTestAppContext,
                        window: AnyWindowHandle,
                        tab: &gpui::Entity<FileTab>,
                        n: &str| {
        frame(cx, window);
        let button = cx
            .read(|cx| tab.read(cx).preview_bounds())
            .expect("Preview is drawn");
        cx.simulate_click(window, button.center(), Modifiers::none());
        frame(cx, window);
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
        frame(cx, window);
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
        frame(cx, window);
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
                frame(cx, window);
                press_copy(cx, window, &tab, sh_row, &n);
                cx.advance_clock(COPIED - Duration::from_millis(500));
                frame(cx, window);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).copied()),
                    Some(Copied::Code(sh_row)),
                    "{n}: still Copied 1 s after the second press"
                );
                cx.advance_clock(Duration::from_millis(600));
                frame(cx, window);
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
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };

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
                frame(cx, window);
                let button = cx
                    .read(|cx| tab.read(cx).preview_bounds())
                    .expect("Preview is drawn");
                cx.simulate_click(window, button.center(), Modifiers::none());
                frame(cx, window);
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
                    frame(cx, window);
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
    let show = move |cx: &mut VisualTestAppContext,
                     window: AnyWindowHandle,
                     tab: &gpui::Entity<FileTab>,
                     row: usize| {
        cx.read(|cx| tab.read(cx).preview_list())
            .scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.0),
            });
        frame(cx, window);
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
                preview(cx, window, &tab);
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
                frame(cx, window);
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
                preview(cx, window, &tab);
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
                preview(cx, window, &tab);
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
                frame(cx, window);
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
                preview(cx, window, &tab);
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
