//! The file tab's Markdown Preview (M11ac; M11ai's reader): its body, the Mermaid blocks it
//! shows, its links and its scrollbar's handlers (moved out of `file_tab.rs`, which keeps the
//! fields, `preview_shown` and the switch).

use super::*;

impl FileTab {
    /// A link clicked in the Preview (M11ac, `md_preview::link_target`): a web or mail link
    /// opens in the default browser, a file opens in a tab (if it is one); nothing else.
    fn follow_link(&mut self, url: &str, cx: &mut Context<Self>) {
        let base = self.path.parent().unwrap_or(Path::new("/"));
        match link_target(base, url) {
            LinkTarget::Web(url) => cx.open_url(&url),
            LinkTarget::File(path) if path.is_file() => cx.emit(FileTabEvent::OpenFile(path)),
            LinkTarget::File(_) | LinkTarget::Nowhere => {}
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
                self.parsed = Some((version, read.clone()));
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
                code,
                copied: self.copied,
                on_copy: {
                    let view = cx.entity().downgrade();
                    Rc::new(move |row, _, cx| {
                        view.update(cx, |this, cx| this.copy_code(row, cx)).ok();
                    })
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
            }),
        };
        let (bar, ink, state) = (self.scrollbar, theme.ui().text, self.preview_list.clone());
        let view = cx.entity().downgrade();
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
            // M11ae: the scrollbar over the list: its thumb (the fade read from the clock; the
            // fade task redraws), and the 12 pt edge zone's presses, taken before the list's
            // own (bubbling goes last-registered first); presses elsewhere reach the list.
            .child(
                canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                    move |_, hitbox, window, _| {
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
                    },
                )
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
        self.copy_gen += 1;
        let press = self.copy_gen;
        self.copied = Some(md_preview::Copied::Code(row));
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
