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

    /// The Preview's body (M11ac): the file's Markdown drawn as a reader, a GPUI `list` of
    /// its blocks (only those in sight are drawn). The text is parsed again only for a new
    /// version; the list then keeps its place by block.
    pub(super) fn preview_body(&mut self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = self.doc() else {
            return div().flex_1().into_any_element();
        };
        let version = doc.version;
        let blocks = match &self.parsed {
            Some((v, blocks)) if *v == version => blocks.clone(),
            _ => {
                let blocks = Rc::new(crate::agent_markdown::parse(&doc.text));
                // A new list (its measuring is by the block count), at the place kept.
                let top = self.preview_list.logical_scroll_top();
                self.preview_list = preview_list_of(blocks.len(), cx.entity().downgrade());
                self.preview_list.scroll_to(top);
                self.parsed = Some((version, blocks.clone()));
                blocks
            }
        };
        let mermaid = self.mermaid_shown(version, &blocks, theme.is_dark(), cx);
        let view = cx.entity().downgrade();
        let draw = md_preview::Draw {
            colours: md_preview::Colours::of(&theme),
            code_font: cx.global::<Appearance>().font.family.clone(),
            on_link: Rc::new(move |url, _, cx| {
                view.update(cx, |this, cx| this.follow_link(&url, cx)).ok();
            }),
            links: Cell::new(0),
            mermaid,
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
                list(self.preview_list.clone(), move |ix, _, _| {
                    md_preview::row(&blocks, ix, &draw)
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
        blocks: &Rc<Vec<Md>>,
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
