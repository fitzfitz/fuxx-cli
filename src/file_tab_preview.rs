//! The file tab's Markdown Preview (M11ac; M11ai's reader): its body, the Mermaid blocks it
//! shows, its links and its scrollbar's handlers (moved out of `file_tab.rs`, which keeps the
//! fields, `preview_shown` and the switch).

use super::*;
use crate::ui_tokens::size::TERMINAL_PADDING;

impl FileTab {
    /// A link clicked in the Preview (M11ac, `md_preview::link_target`): a web or mail link
    /// opens in the default browser, a file opens in a tab (if it is one); M11ai: a `#part` of
    /// this file jumps to its heading (an unknown one does nothing), and a Markdown file's part
    /// goes with it (the workspace opens it in Preview there; another file's is dropped).
    fn follow_link(&mut self, url: &str, cx: &mut Context<Self>) {
        // The scenes' frozen tabs record every link followed, and open no browser.
        #[cfg(feature = "ui-shots")]
        if self.frozen {
            self.followed = Some(url.to_string());
        }
        let base = self.path.parent().unwrap_or(Path::new("/"));
        match link_target(base, url) {
            #[cfg(feature = "ui-shots")]
            LinkTarget::Web(_) if self.frozen => {}
            LinkTarget::Web(url) => cx.open_url(&url),
            LinkTarget::File(path, part) if path.is_file() => {
                let part = part.filter(|_| is_markdown(&path));
                cx.emit(FileTabEvent::OpenFile(path, part))
            }
            LinkTarget::Here(anchor) => {
                let row = self
                    .parsed
                    .as_ref()
                    .and_then(|(_, read)| read.plan.anchor_row(&anchor));
                if let Some(row) = row {
                    self.jump_to_row(row, cx);
                }
            }
            LinkTarget::File(..) | LinkTarget::Nowhere => {}
        }
    }

    /// Preview on, at heading `anchor` (M11ai: a link to it from another file; the workspace
    /// saves the Preview): the jump is made now if the file as it is has been read, else where
    /// it is read (`preview_body`, plan-time call 5). An unknown heading: the top.
    pub fn show_anchor(&mut self, anchor: String, cx: &mut Context<Self>) {
        if !self.preview {
            self.preview = true;
            // As Preview on by hand: Follow ends, the find bar and the selection go.
            self.live.follow = false;
            self.mark_tip = None;
            self.find = None;
            self.selection = None;
            self.selecting = false;
            self.refresh_place(cx);
        }
        let version = self.doc().map(|d| d.version);
        let read = match &self.parsed {
            Some((v, read)) if Some(*v) == version => Some(read.clone()),
            _ => None,
        };
        match read {
            Some(read) => {
                self.pending_anchor = None;
                if let Some(row) = read.plan.anchor_row(&anchor) {
                    self.jump_to_row(row, cx);
                }
            }
            None => self.pending_anchor = Some(anchor),
        }
        cx.notify();
    }

    /// Jump to list row `row` (M11ai: a heading): the row's content `JUMP_ROOM` under the
    /// Preview's top (`md_reader::jump_offset` of the space over it); a jump is a scroll, so the
    /// scrollbar shows. Task 8: a heading in "On this page" is its current entry at once, and
    /// the entry is checked again once the jump is laid out.
    pub(super) fn jump_to_row(&mut self, row: usize, cx: &mut Context<Self>) {
        self.place_at_row(row, cx);
        if let Some(entry) = self.outline_rows().iter().position(|r| *r == row) {
            self.outline_current = Some(entry);
        }
        cx.notify();
    }

    /// `jump_to_row` without the redraw (`preview_body` jumps in the frame it draws), nor
    /// setting the current entry (worked out after it is laid out).
    fn place_at_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some((_, read)) = &self.parsed else {
            return;
        };
        let room = md_preview::row_top(read, &md_preview::Sizes::reader(), row);
        self.preview_list.scroll_to(ListOffset {
            item_ix: row,
            offset_in_item: px(crate::md_reader::jump_offset(room)),
        });
        self.track_due.set(true);
        self.scrolled(cx);
    }

    /// Each "On this page" entry's list row (none without an outline).
    fn outline_rows(&self) -> Vec<usize> {
        let Some((_, read)) = &self.parsed else {
            return Vec::new();
        };
        let plan = &read.plan;
        plan.outline
            .iter()
            .map(|e| plan.row_of_block(plan.headings[e.heading].block))
            .collect()
    }

    /// Which "On this page" entry is current (M11ai Task 8, plan-time call 12), from the list
    /// as it is: the last heading reached (above the list's top row, or its top within `REACH`
    /// of the view's height under the view's top); scrolled to the very end, the last heading
    /// in view (drawn, its top over the view's bottom). None until the first is reached. Run
    /// at the start of a frame (`track_next_frame`), never while drawing; redraws only when
    /// the entry changes.
    pub(super) fn track_heading(&mut self, cx: &mut Context<Self>) {
        if !self.preview_shown() {
            return;
        }
        let rows = self.outline_rows();
        let list = &self.preview_list;
        let view = list.viewport_bounds();
        let top_row = list.logical_scroll_top().item_ix;
        let reach = view.top() + view.size.height * crate::ui_tokens::reader::REACH;
        let now = crate::md_reader::current_entry(
            &rows,
            list.is_scrolled_to_end() == Some(true),
            |r| r < top_row || list.bounds_for_item(r).is_some_and(|b| b.top() <= reach),
            |r| {
                list.bounds_for_item(r)
                    .is_some_and(|b| b.top() < view.bottom())
            },
        );
        if now != self.outline_current {
            self.outline_current = now;
            cx.notify();
        }
    }

    /// Whether "On this page" shows now (M11ai Task 8): `Some(true)` as the column at the
    /// Preview's right (wide enough, `outline_fits`), `Some(false)` not (narrower), None when
    /// there is no outline (fewer than two entries) or no Preview.
    pub fn outline_shown(&self) -> Option<bool> {
        let (_, read) = self.parsed.as_ref()?;
        if !self.preview_shown() || read.plan.outline.is_empty() {
            return None;
        }
        Some(self.outline_fits.get() == Some(true))
    }

    /// An "On this page" entry clicked: a jump to its heading.
    fn outline_jump(&mut self, entry: usize, cx: &mut Context<Self>) {
        if let Some(row) = self.outline_rows().get(entry).copied() {
            self.jump_to_row(row, cx);
        }
    }

    /// A press at `p` in the Preview's scrollbar zone (M11ae): the thumb is grabbed, or jumps to
    /// centre on the press and is dragged on, as in the code view. Whether it was in the zone
    /// (then the press goes no further: the list's links and diagrams get the rest).
    fn preview_press(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(track) = preview_track(&self.preview_list).filter(|t| t.zone_has(p)) else {
            return false;
        };
        let (at, t) = (track.along(p), track.thumb);
        // Frozen content height for the drag, before the first move of the list.
        self.preview_list.scrollbar_drag_started();
        let grab = if (t.start..=t.start + t.len).contains(&at) {
            at - t.start
        } else {
            self.preview_scroll_to(&track, at - t.len / 2.0, cx);
            t.len / 2.0
        };
        self.scrollbar.start_drag(Axis::Vertical, grab);
        cx.notify();
        true
    }

    /// Scroll the list so the thumb starts at `start` along `track` (`set_offset_from_scrollbar`
    /// takes the offset with a negative `y`).
    fn preview_scroll_to(&mut self, track: &Track, start: f32, cx: &mut Context<Self>) {
        let to = scrollbar::offset_for(start, track.viewport, track.content);
        if to >= track.content - track.viewport - 1.0 {
            // The thumb at its end: the text's true end, whatever rows were not yet measured.
            self.preview_list.scroll_to_end();
        } else if self.preview_list.item_count() > PREVIEW_MEASURE_ALL {
            let item_ix = (to / PREVIEW_BLOCK_HINT) as usize;
            self.preview_list.scroll_to(ListOffset {
                item_ix,
                offset_in_item: px(0.0),
            });
        } else {
            self.preview_list
                .set_offset_from_scrollbar(point(px(0.0), px(-to)));
        }
        // M11ai: the current outline entry, once this scroll is laid out.
        self.track_due.set(true);
        self.scrolled(cx);
        cx.notify();
    }

    /// The pointer moved anywhere in the window while Preview shows: a thumb drag follows it;
    /// near the list's right edge the scrollbar shows.
    fn preview_move(&mut self, e: &MouseMoveEvent, over: bool, cx: &mut Context<Self>) {
        if let Some((_, grab)) = self.scrollbar.drag() {
            if e.pressed_button != Some(MouseButton::Left) {
                self.preview_release(cx);
            } else if let Some(track) = preview_track(&self.preview_list) {
                self.preview_scroll_to(&track, track.along(e.position) - grab, cx);
            }
            return;
        }
        let hot = over && preview_track(&self.preview_list).is_some_and(|t| t.zone_has(e.position));
        if self.scrollbar.set_hot(hot) {
            self.scrolled(cx);
            cx.notify();
        }
    }

    /// The button released anywhere in the window: a thumb drag ends.
    fn preview_release(&mut self, cx: &mut Context<Self>) {
        if self.scrollbar.end_drag(Instant::now()) {
            self.preview_list.scrollbar_drag_ended();
            self.scrolled(cx);
            cx.notify();
        }
    }

    /// The Preview's body (M11ac, M11ai): the file's Markdown drawn as a reader, a GPUI `list`
    /// of its rows (the front matter, if any, then a row per block; only those in sight are
    /// drawn). The text is read again (`md_reader::read`: front matter, parse, plan) only for a
    /// new version; the list then keeps its place by row.
    pub(super) fn preview_body(&mut self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = self.doc() else {
            return div().flex_1().into_any_element();
        };
        let version = doc.version;
        let read = match &self.parsed {
            Some((v, read)) if *v == version => read.clone(),
            _ => {
                let (blocks, plan) = crate::md_reader::read(&doc.text);
                let read = Rc::new(md_preview::ReaderDoc { blocks, plan });
                // A new list (its measuring is by the row count), at the place kept.
                let top = self.preview_list.logical_scroll_top();
                self.preview_list = preview_list_of(read.plan.rows(), cx.entity().downgrade());
                self.preview_list.scroll_to(top);
                // M11ai: "On this page"'s own list (its scroll kept while the entries are as
                // many), and its current entry checked again once the new list is laid out.
                let entries = read.plan.outline.len();
                if self.outline_list.item_count() != entries {
                    self.outline_list =
                        ListState::new(entries, ListAlignment::Top, px(PREVIEW_OVERDRAW));
                }
                self.track_due.set(true);
                self.parsed = Some((version, read.clone()));
                // M11ai: a heading asked for before the file was read (`show_anchor`).
                if let Some(row) = self
                    .pending_anchor
                    .take()
                    .and_then(|anchor| read.plan.anchor_row(&anchor))
                {
                    self.place_at_row(row, cx);
                }
                read
            }
        };
        let mermaid = self.mermaid_shown(version, &read.blocks, theme.is_dark(), cx);
        // M11ai: the code blocks' colours for this version and theme: what the cache has at
        // once, the rest asked of the background (plan-time call 5: started where the parse
        // is, as the Mermaid queue is; the answer comes with a notify).
        if self.code_for != Some((version, theme)) {
            self.colour_code(cx);
        }
        let code = match self.code_for {
            Some(for_) if for_ == (version, theme) => self.code_looks.clone(),
            _ => Rc::default(),
        };
        // M11ai Task 8: "On this page" as a column when there is an outline and the Preview was
        // last laid out wide enough for it (`outline_fits`, decided in the canvas's prepaint).
        let has_outline = !read.plan.outline.is_empty();
        let column = (self.outline_shown() == Some(true))
            .then(|| self.outline_column(&read, md_preview::Colours::of(&theme), cx));
        #[cfg(feature = "ui-shots")]
        if column.is_none() {
            self.outline_drawn.borrow_mut().clear();
        }
        let view = cx.entity().downgrade();
        let draw = md_preview::Draw {
            colours: md_preview::Colours::of(&theme),
            code_font: cx.global::<Appearance>().font.family.clone(),
            on_link: Rc::new(move |url, _, cx| {
                view.update(cx, |this, cx| this.follow_link(&url, cx)).ok();
            }),
            links: Cell::new(0),
            mermaid,
            sizes: md_preview::Sizes::reader(),
            reader: Some(md_preview::ReaderDraw {
                doc: read,
                measure: crate::ui_tokens::reader::MEASURE,
                aside: if column.is_some() {
                    crate::ui_tokens::reader::OUTLINE_W + crate::ui_tokens::reader::OUTLINE_GAP
                } else {
                    0.0
                },
                code,
                copied: self.copied,
                on_copy: {
                    let view = cx.entity().downgrade();
                    Rc::new(move |row, _, cx| {
                        view.update(cx, |this, cx| this.copy_code(row, cx)).ok();
                    })
                },
                on_anchor_copy: {
                    let view = cx.entity().downgrade();
                    Rc::new(move |row, _, cx| {
                        view.update(cx, |this, cx| this.copy_anchor(row, cx)).ok();
                    })
                },
                #[cfg(feature = "ui-shots")]
                anchor_drawn: {
                    self.anchor_drawn.borrow_mut().clear();
                    self.anchor_drawn.clone()
                },
                #[cfg(feature = "ui-shots")]
                drawn: {
                    self.reader_drawn.borrow_mut().clear();
                    self.reader_drawn.clone()
                },
                #[cfg(feature = "ui-shots")]
                code_drawn: {
                    self.code_drawn.borrow_mut().clear();
                    self.code_drawn.clone()
                },
                #[cfg(feature = "ui-shots")]
                callout_drawn: {
                    self.callout_drawn.borrow_mut().clear();
                    self.callout_drawn.clone()
                },
                #[cfg(feature = "ui-shots")]
                front_drawn: {
                    self.front_drawn.borrow_mut().take();
                    self.front_drawn.clone()
                },
                #[cfg(feature = "ui-shots")]
                pills_drawn: {
                    self.pills_drawn.borrow_mut().clear();
                    self.pills_drawn.clone()
                },
            }),
        };
        let (bar, ink, state) = (self.scrollbar, theme.ui().text, self.preview_list.clone());
        let view = cx.entity().downgrade();
        let prepaint = {
            let (fits, width, due) = (
                self.outline_fits.clone(),
                self.preview_width.clone(),
                self.track_due.clone(),
            );
            let view = view.clone();
            // M11ai Task 8 (plan-time calls 12, 13): the Preview's width as laid out decides
            // whether the outline's column fits (a layout cache, as `fits_side`): when that
            // changes, the tab is drawn again once (its rows measured again if the reader's
            // area changed). A width change, or a check asked for since the last layout (a
            // remeasure, a reload, a jump, a scrollbar drag), works out the current entry at
            // the start of the next frame, once, from this layout.
            move |bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
                let fit = outline_fits(f32::from(bounds.size.width));
                let was = fits.get();
                if was != Some(fit) {
                    fits.set(Some(fit));
                    let reflow = has_outline && (was == Some(true)) != fit;
                    let view = view.clone();
                    cx.defer(move |cx| {
                        view.update(cx, |this, cx| {
                            if reflow {
                                this.preview_list.remeasure();
                                this.track_due.set(true);
                            }
                            cx.notify();
                        })
                        .ok();
                    });
                }
                if width.get() != Some(bounds.size.width) {
                    width.set(Some(bounds.size.width));
                    due.set(true);
                }
                if due.take() {
                    track_next_frame(view.clone(), window);
                }
                window.insert_hitbox(bounds, HitboxBehavior::Normal)
            }
        };
        div()
            .id("file-md-preview")
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(
                list(self.preview_list.clone(), move |row, _, _| {
                    md_preview::reader_row(row, &draw)
                })
                .size_full(),
            )
            .children(column)
            // M11ae: the scrollbar over the list: its thumb (the fade read from the clock; the
            // fade task redraws), and the 12 pt edge zone's presses, taken before the list's
            // own (bubbling goes last-registered first); presses elsewhere reach the list.
            .child(
                canvas(prepaint, move |_, hitbox, window, _| {
                    let now = Instant::now();
                    if bar.is_shown(now)
                        && let Some(t) = preview_track(&state)
                    {
                        scrollbar::paint(
                            window,
                            t.area,
                            Axis::Vertical,
                            &t.thumb,
                            bar.alpha_at(now),
                            ink,
                        );
                    }
                    window.on_mouse_event({
                        let view = view.clone();
                        move |e: &MouseDownEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble
                                && e.button == MouseButton::Left
                                && view
                                    .update(cx, |this, cx| this.preview_press(e.position, cx))
                                    .unwrap_or(false)
                            {
                                cx.stop_propagation();
                            }
                        }
                    });
                    window.on_mouse_event({
                        let view = view.clone();
                        move |e: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble {
                                let over = hitbox.is_hovered(window);
                                view.update(cx, |this, cx| this.preview_move(e, over, cx))
                                    .ok();
                            }
                        }
                    });
                    window.on_mouse_event({
                        let view = view.clone();
                        move |_: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble {
                                view.update(cx, |this, cx| this.preview_release(cx)).ok();
                            }
                        }
                    });
                    // The pointer left the window (no last move comes): not hot.
                    window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble {
                            view.update(cx, |this, cx| this.pointer_left(cx)).ok();
                        }
                    });
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }

    /// How each Mermaid block of `blocks` (version `version`) is shown this frame (M11ac Part
    /// 2): planned once per version and light or dark; each diagram read from the cache, or
    /// queued to render (its code shown meanwhile; when it is ready the rows are measured again
    /// and drawn). A file with none costs nothing past the plan.
    fn mermaid_shown(
        &mut self,
        version: u64,
        blocks: &[Md],
        dark: bool,
        cx: &mut Context<Self>,
    ) -> Rc<Vec<Option<md_preview::Shown>>> {
        use md_preview::Shown;
        use mermaid::Plan;
        let plan = match &self.mermaid_plan {
            Some((v, d, plan)) if *v == version && *d == dark => plan.clone(),
            _ => {
                let plan = Rc::new(mermaid::plan(blocks, dark));
                self.mermaid_plan = Some((version, dark, plan.clone()));
                plan
            }
        };
        if plan.iter().all(Option::is_none) {
            self.mermaid_shown = Rc::default();
            return self.mermaid_shown.clone();
        }
        let view = cx.entity().downgrade();
        let who = cx.entity_id().as_u64();
        let shown: Vec<Option<Shown>> = plan
            .iter()
            .zip(blocks.iter())
            .map(|(plan, md)| {
                Some(match plan.as_ref()? {
                    Plan::TooMany => Shown::Code(Some(mermaid::note(mermaid::TOO_MANY))),
                    Plan::TooLarge => Shown::Code(Some(mermaid::note(mermaid::TOO_LARGE))),
                    Plan::Diagram(key) => {
                        let Md::Code { text, .. } = md else {
                            return None;
                        };
                        let (tell_view, wanted_view) = (view.clone(), view.clone());
                        let waiter = Waiter {
                            tell: Box::new(move |cx: &mut App| {
                                tell_view
                                    .update(cx, |this, cx| {
                                        this.preview_list.remeasure();
                                        this.track_due.set(true);
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                            // Still wanted: the tab is open, shows its Preview, and its plan
                            // (this file version, the theme's light or dark) has the diagram.
                            wanted: Box::new(move |cx: &App, key: &Key| {
                                wanted_view
                                    .upgrade()
                                    .is_some_and(|tab| tab.read(cx).wants_mermaid(key, cx))
                            }),
                        };
                        match Mermaids::shown(*key, text, who, waiter, cx) {
                            Asked::Ready(Drawn { image, w, h }) => Shown::Diagram { image, w, h },
                            Asked::Failed(e) => Shown::Code(Some(mermaid::note(&e))),
                            Asked::TooMuch => Shown::Code(Some(mermaid::note(mermaid::TOO_MUCH))),
                            Asked::Crowded => Shown::Code(Some(mermaid::note(mermaid::CROWDED))),
                            Asked::Waiting => Shown::Code(None),
                        }
                    }
                })
            })
            .collect();
        self.mermaid_shown = Rc::new(shown);
        self.mermaid_shown.clone()
    }

    /// The Preview's code blocks coloured (M11ai) for the version parsed (if it is the one
    /// read) in the current theme: every block with a language that is not a diagram to draw
    /// takes its look from the cache (by language and code text, in this theme) at once; the
    /// rest are coloured on the background executor with the shared highlighter, and the answer
    /// is taken only if the version parsed and the theme are still the ones asked for (else a
    /// newer job has been started). The cache keeps only this version's blocks.
    pub(super) fn colour_code(&mut self, cx: &mut Context<Self>) {
        use crate::md_reader::{code_runs, fence_lang};
        let Some((version, read)) = self.parsed.clone() else {
            return;
        };
        if self.doc().map(|d| d.version) != Some(version) {
            // A new version, not parsed yet: its parse starts the job.
            return;
        }
        let theme = cx.global::<Appearance>().theme;
        if self.code_cache_theme != Some(theme) {
            self.code_cache.clear();
            self.code_cache_theme = Some(theme);
        }
        let diagrams = mermaid::plan(&read.blocks, theme.is_dark());
        let (mut looks, mut todo, mut keep) = (HashMap::new(), Vec::new(), BTreeSet::new());
        for (b, md) in read.blocks.iter().enumerate() {
            let Md::Code { lang, text } = md else {
                continue;
            };
            let Some(lang) = fence_lang(lang.as_deref()) else {
                continue;
            };
            if matches!(diagrams.get(b), Some(Some(mermaid::Plan::Diagram(_)))) {
                continue;
            }
            let key = (lang.to_string(), text.clone());
            match self.code_cache.get(&key) {
                Some(Some(look)) => {
                    looks.insert(b, look.clone());
                }
                Some(None) => {}
                None => todo.push((b, key.clone())),
            }
            keep.insert(key);
        }
        self.code_cache.retain(|key, _| keep.contains(key));
        self.code_looks = Rc::new(looks);
        self.code_for = Some((version, theme));
        if todo.is_empty() {
            return;
        }
        // Not loaded yet: `colour` runs this again once they are.
        let Some(h) = self.shared_highlighter(cx) else {
            return;
        };
        let fill = md_preview::Colours::of(&theme).code_bg;
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    todo.into_iter()
                        .map(|(b, (lang, text))| {
                            let look = h.highlight_lang(&lang, &text).map(|lines| {
                                code_runs(&text, &lines, |c| ensure_contrast(c, fill, TEXT_MIN))
                            });
                            (b, (lang, text), look)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |this, cx| {
                let now = cx.global::<Appearance>().theme;
                if now != theme || this.code_for != Some((version, theme)) {
                    return;
                }
                let mut looks = (*this.code_looks).clone();
                for (b, key, look) in done {
                    let look = look.map(|(shown, runs)| {
                        Rc::new(md_preview::CodeLook {
                            shown: shown.into(),
                            runs,
                        })
                    });
                    if let Some(look) = &look {
                        looks.insert(b, look.clone());
                    }
                    this.code_cache.insert(key, look);
                }
                this.code_looks = Rc::new(looks);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A code block's Copy pressed (M11ai), on list row `row`: its code, as written in the file
    /// (tabs kept), on the clipboard, and its button reads "Copied" for `reader::COPIED` (a
    /// one-shot timer, which leaves it if another press came since).
    fn copy_code(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some((_, read)) = &self.parsed else {
            return;
        };
        let crate::md_reader::RowKind::Block(b) = read.plan.row_kind(row) else {
            return;
        };
        let Some(Md::Code { text, .. }) = read.blocks.get(b) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        self.show_copied(md_preview::Copied::Code(row), cx);
    }

    /// A heading's `#` pressed (M11ai), on list row `row`: the link to it on the clipboard
    /// (`md_reader::heading_link`: from its repo's top folder, else from home, plan-time call
    /// 10), and the `#` reads "Copied" as a code block's Copy does.
    fn copy_anchor(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some((_, read)) = &self.parsed else {
            return;
        };
        let Some(heading) = read
            .plan
            .headings
            .iter()
            .find(|h| read.plan.row_of_block(h.block) == row)
        else {
            return;
        };
        let rel = self.git_place.as_ref().map(|p| p.rel.as_path());
        let home = crate::workspace::home_dir();
        let link = crate::md_reader::heading_link(rel, &self.path, home.as_deref(), &heading.slug);
        cx.write_to_clipboard(ClipboardItem::new_string(link));
        self.show_copied(md_preview::Copied::Anchor(row), cx);
    }

    /// `what` reads "Copied" for `reader::COPIED` (a one-shot timer, which leaves it if another
    /// press came since).
    fn show_copied(&mut self, what: md_preview::Copied, cx: &mut Context<Self>) {
        self.copy_gen += 1;
        let press = self.copy_gen;
        self.copied = Some(what);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(crate::ui_tokens::reader::COPIED)
                .await;
            this.update(cx, |this, cx| {
                if this.copy_gen == press {
                    this.copied = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Whether this tab still wants diagram `key` drawn (M11ac Part 2, asked before it
    /// renders): Preview shows, the plan for the file as read has it, and the theme is still
    /// light or dark as the key says.
    fn wants_mermaid(&self, key: &Key, cx: &App) -> bool {
        let dark = cx.global::<Appearance>().theme.is_dark();
        key.dark == dark
            && self.preview_shown()
            && self.mermaid_plan.as_ref().is_some_and(|(_, d, plan)| {
                *d == dark && plan.contains(&Some(mermaid::Plan::Diagram(*key)))
            })
    }
}

/// Whether "On this page" shows as a column in a Preview `width` wide (M11ai Task 8, plan-time
/// call 13): room for the prose column, the side padding each side, the gap and the column.
pub(super) fn outline_fits(width: f32) -> bool {
    use crate::ui_tokens::reader::{MEASURE, OUTLINE_GAP, OUTLINE_W};
    width >= MEASURE + 2.0 * TERMINAL_PADDING.0 + OUTLINE_GAP + OUTLINE_W
}

/// "On this page"'s current entry worked out at the start of the next frame (`track_heading`):
/// out of any borrow of the list, and after the layout drawn last.
pub(super) fn track_next_frame(view: WeakEntity<FileTab>, window: &Window) {
    window.on_next_frame(move |_, cx| {
        view.update(cx, |this, cx| this.track_heading(cx)).ok();
    });
}

impl FileTab {
    /// "On this page" as a column (M11ai Task 8): `OUTLINE_W` wide, `TERMINAL_PADDING` from the
    /// Preview's right and its top; a dim caption, then the entries in a list of their own
    /// (only those in sight drawn; its own scroll, the wheel over it not reaching the text).
    /// Each entry is an `OUTLINE_ROW` row of `OUTLINE_TEXT` text on one line (a long one ends
    /// in "…", its full text in a tooltip), a level-3 heading's `OUTLINE_INDENT` in; the
    /// current one in the accent with an `OUTLINE_BAR` bar on its left, the rest dim; a
    /// hovered one on the panel hover fill. A click jumps to its heading.
    fn outline_column(
        &self,
        read: &Rc<md_preview::ReaderDoc>,
        c: md_preview::Colours,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::ui_tokens::reader::{
            OUTLINE_BAR, OUTLINE_INDENT, OUTLINE_PAD, OUTLINE_ROW, OUTLINE_TEXT, OUTLINE_W,
        };
        let plan = &read.plan;
        let entries: Rc<Vec<(SharedString, bool)>> = Rc::new(
            plan.outline
                .iter()
                .map(|e| (plan.headings[e.heading].text.clone().into(), e.indent))
                .collect(),
        );
        let current = self.outline_current;
        let view = cx.entity().downgrade();
        #[cfg(feature = "ui-shots")]
        let probe = {
            self.outline_drawn.borrow_mut().clear();
            self.outline_drawn.clone()
        };
        let caption = div()
            .h(px(OUTLINE_ROW))
            .pl(px(OUTLINE_PAD))
            .flex()
            .items_center()
            .text_color(hsla(c.dim))
            .child("On this page");
        let rows = list(self.outline_list.clone(), move |i, _, _| {
            let Some((text, indent)) = entries.get(i).cloned() else {
                return div().into_any_element();
            };
            let on = current == Some(i);
            let view = view.clone();
            let tip = text.clone();
            let inset = OUTLINE_PAD - OUTLINE_BAR + if indent { OUTLINE_INDENT } else { 0.0 };
            let entry = div()
                .id(("md-outline", i))
                .h(px(OUTLINE_ROW))
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .cursor_pointer()
                .hover(move |d| d.bg(hsla(c.outline_hover)))
                .child(
                    div()
                        .w(px(OUTLINE_BAR))
                        .h_full()
                        .when(on, |d| d.bg(hsla(c.outline_bar))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .pl(px(inset))
                        .truncate()
                        .text_color(hsla(if on { c.outline_current } else { c.outline_dim }))
                        .child(text),
                )
                .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone())).into() })
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| this.outline_jump(i, cx)).ok();
                });
            let row = div().w_full().child(entry);
            #[cfg(feature = "ui-shots")]
            let row = {
                let probe = probe.clone();
                row.on_children_prepainted(move |bounds, _, _| {
                    if let Some(b) = bounds.first() {
                        probe.borrow_mut().insert(i, *b);
                    }
                })
            };
            row.into_any_element()
        })
        .flex_1()
        .min_h(px(0.0))
        .w_full();
        div()
            .id("md-outline")
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(TERMINAL_PADDING.0))
            .w(px(OUTLINE_W))
            .pt(px(TERMINAL_PADDING.1))
            .flex()
            .flex_col()
            .text_size(px(OUTLINE_TEXT))
            .occlude()
            .child(caption)
            .child(rows)
            .into_any_element()
    }
}

/// What the scenes read of "On this page" (M11ai Task 8).
#[cfg(feature = "ui-shots")]
impl FileTab {
    /// The current entry (None until the first heading is reached).
    pub fn outline_current(&self) -> Option<usize> {
        self.outline_current
    }

    /// Entry `entry`'s row in the column as last drawn (only those in sight; none while the
    /// column does not show).
    pub fn outline_entry_bounds(&self, entry: usize) -> Option<Bounds<Pixels>> {
        self.outline_drawn.borrow().get(&entry).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_tokens::reader::{MEASURE, OUTLINE_GAP, OUTLINE_W};

    /// "On this page" shows as a column (plan-time call 13) from the prose column, the side
    /// padding each side, the gap and the outline (760 pt with today's padding).
    #[test]
    fn the_outline_column_shows_from_the_column_padding_gap_and_outline() {
        let from = MEASURE + 2.0 * TERMINAL_PADDING.0 + OUTLINE_GAP + OUTLINE_W;
        assert!(outline_fits(from));
        assert!(outline_fits(1200.0));
        assert!(!outline_fits(from - 1.0));
        assert!(!outline_fits(700.0));
    }
}
