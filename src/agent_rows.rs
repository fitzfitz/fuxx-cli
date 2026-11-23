//! The agent tab's rows (moved out of `agent_tab.rs` for M11ar, Task 0; no change): an item's
//! row, its body by kind, tool rows, diffs, the permission card and your message's text.

use super::*;

impl AgentTab {
    /// Row `k` of the shown view's list (M11ar: each view has its own rows: `agent_chat::rows_of`).
    pub(super) fn item_row(&self, k: usize, look: &Look) -> AnyElement {
        use crate::agent_chat::{
            agent_side, avatar_top, chat, gap_before_at, shows_avatar_at, shows_thinking,
        };
        let items = self.conversation.items();
        let pane = self.pane();
        // A lane's view: its task first ("from Main"), how it ended last (M11ar).
        if pane.head && k == 0 {
            return self.lane_head_row(look);
        }
        let k = k - pane.head as usize;
        let order = &pane.order;
        let Some(&ix) = order.get(k) else {
            return self.lane_end_row(look);
        };
        let item = &items[ix];
        let c = &look.c;
        let last = k + 1 == order.len();
        let thinking = last
            && match &self.view {
                crate::agent_lanes::View::Main => {
                    shows_thinking(self.status() == Status::Working, items)
                }
                crate::agent_lanes::View::Lane(id) => self.conversation.lane(id).is_some_and(|l| {
                    crate::agent_chat::shows_thinking_in(l.status.live(), items, order)
                }),
            };
        // Its text's pieces are keyed by this row (chat selection, chore after M11ar).
        if let Some((select, _)) = &look.draw.select {
            select.begin_row(k);
        }
        let body = self.item_body(ix, item, 0, look);
        let text_column = chat::AVATAR + chat::AVATAR_GAP;
        let column = div().w_full().max_w(px(chat::COLUMN_MAX));
        let column = if agent_side(item) {
            let gutter = div()
                .flex_none()
                .w(px(chat::AVATAR))
                .when(shows_avatar_at(items, order, k), |d| {
                    d.pt(px(avatar_top(item))).child(self.avatar(look))
                });
            column
                .flex()
                .flex_row()
                .gap(px(chat::AVATAR_GAP))
                .child(self.marked("column", gutter))
                .child(div().flex_1().min_w(px(0.)).child(body))
        } else if matches!(item, Item::Notice(_) | Item::Error(_)) {
            column.pl(px(text_column)).child(body)
        } else {
            column.child(body)
        };
        // "Cultivating…" under the last row while the turn works and nothing else moves (spec
        // B-B): after your message with the avatar, else in the text column; fading in late,
        // so short gaps never flash it.
        let thinking = thinking.then(|| {
            let after_you = matches!(item, Item::You { .. });
            let gap = match item {
                Item::You { .. } => chat::GAP_TURN,
                Item::Tool { .. } => chat::GAP_TEXT_TOOL,
                _ => chat::GAP_ITEM,
            };
            let slot = div()
                .flex_none()
                .w(px(chat::AVATAR))
                .flex()
                .justify_center()
                .when(after_you, |d| d.child(self.avatar(look)));
            div()
                .w_full()
                .max_w(px(chat::COLUMN_MAX))
                .pt(px(gap))
                .h(px(gap + chat::THINKING_H))
                .flex()
                .items_center()
                .gap(px(chat::AVATAR_GAP))
                .child(slot)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(text::META))
                        .text_color(hsla(c.dim))
                        .child(self.thinking_mark.clone())
                        .child(crate::agent_chat::THINKING_WORD),
                )
                .with_animation(
                    ("agent-thinking", ix),
                    Animation::new(Duration::from_millis(chat::THINKING_FADE_MS)),
                    |d, t| d.opacity(crate::agent_chat::thinking_opacity(t)),
                )
        });
        div()
            // Your message's hover actions show over its whole row (batch 3, chore 2).
            .when(matches!(item, Item::You { .. }), |d| {
                d.group(rewind::YOU_ROW)
            })
            .w_full()
            .px(px(AREA_X))
            .pt(px(gap_before_at(items, order, k)))
            .when(last, |d| d.pb(px(chat::LAST_BOTTOM)))
            // The column centred in the tab (the box under it too: `box_area`).
            .flex()
            .flex_col()
            .items_center()
            .text_size(px(chat::BODY))
            .line_height(relative(chat::BODY_LINE))
            .child(column)
            .children(thinking)
            .into_any_element()
    }

    /// The agent's avatar (spec B-A): its logo bare, else its letters on its colour.
    pub(super) fn avatar(&self, look: &Look) -> AnyElement {
        use crate::agent_chat::chat;
        match look.logo_image.clone() {
            Some(logo) => div()
                .flex_none()
                .size(px(chat::AVATAR))
                .flex()
                .items_center()
                .justify_center()
                .child(gpui::img(logo).size(px(chat::LOGO)))
                .into_any_element(),
            None => div()
                .flex_none()
                .size(px(chat::AVATAR))
                .rounded(px(6.))
                .bg(hsla(Rgb {
                    r: (look.badge_rgb >> 16) as u8,
                    g: (look.badge_rgb >> 8) as u8,
                    b: look.badge_rgb as u8,
                }))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(text::META))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(gpui::white())
                .child(look.badge.clone())
                .into_any_element(),
        }
    }

    /// An item's own look (inside a row, or nested `depth` deep under a tool).
    pub(super) fn item_body(
        &self,
        ix: usize,
        item: &Item,
        depth: usize,
        look: &Look,
    ) -> AnyElement {
        use crate::agent_chat::chat;
        let c = &look.c;
        match item {
            // Your message (spec B-B): a bubble in the colours of the box you typed it in, its
            // tail at the bottom right; attachments as chips under the text.
            Item::You {
                text, attachments, ..
            } => div()
                .w_full()
                .flex()
                .justify_end()
                .child(
                    div().w(relative(0.78)).flex().justify_end().child(
                        div()
                            .relative()
                            .flex()
                            .max_w(px(520.))
                            .child(
                                self.flashed_you(
                                    div()
                                        .max_w(px(520.))
                                        .px(px(14.))
                                        .py(px(9.))
                                        .rounded(px(chat::R_BUBBLE))
                                        .rounded_br(px(chat::R_TAIL))
                                        .bg(hsla(c.you_fill))
                                        .border_1()
                                        .border_color(hsla(c.you_edge))
                                        // The rewind pick's message (batch 3, chore 2): a 2 px focus
                                        // ring, its padding a point less so nothing moves.
                                        .when(self.picked(ix), |d| {
                                            d.border_2()
                                                .border_color(hsla(c.focus_ring))
                                                .px(px(13.))
                                                .py(px(8.))
                                        })
                                        .text_color(hsla(c.you_text))
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.))
                                        .child(self.you_text(ix, text, look))
                                        .when(!attachments.is_empty(), |d| {
                                            d.child(div().flex().flex_wrap().gap(px(6.)).children(
                                                attachments.iter().enumerate().map(|(j, a)| {
                                                    // An image this tab has opens in the viewer.
                                                    let image = self
                                                        .images
                                                        .contains_key(a)
                                                        .then(|| a.clone());
                                                    // A path pill's chip (batch 3, chore 5): the
                                                    // file icon; its file opens, its path is its
                                                    // tooltip.
                                                    let pill = self.sent_pill(a).cloned();
                                                    let glyph = if pill.is_some() {
                                                        Icon::File
                                                    } else {
                                                        Icon::Paperclip
                                                    };
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "you-chip-{ix}-{j}"
                                                        )))
                                                        .when_some(image, |d, name| {
                                                            d.cursor_pointer()
                                                                .hover(move |s| {
                                                                    s.text_color(hsla(c.you_text))
                                                                })
                                                                .on_click(on_tab(
                                                                    &look.view,
                                                                    move |this, cx| {
                                                                        this.view_image(&name, cx)
                                                                    },
                                                                ))
                                                        })
                                                        .when_some(pill, |d, pill| {
                                                            let tip: SharedString = pill
                                                                .path
                                                                .display()
                                                                .to_string()
                                                                .into();
                                                            d.cursor_pointer()
                                                                .hover(move |s| {
                                                                    s.text_color(hsla(c.you_text))
                                                                })
                                                                .tooltip(move |_, cx| {
                                                                    cx.new(|_| {
                                                                        crate::sidebar_toggle::Tip(
                                                                            tip.clone(),
                                                                        )
                                                                    })
                                                                    .into()
                                                                })
                                                                .on_click(on_tab(
                                                                    &look.view,
                                                                    move |this, cx| {
                                                                        this.open_pill(
                                                                            pill.clone(),
                                                                            cx,
                                                                        )
                                                                    },
                                                                ))
                                                        })
                                                        .h(px(22.))
                                                        .px(px(8.))
                                                        .rounded(px(chat::R_CHIP))
                                                        .bg(hsla(c.chip_fill))
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(4.))
                                                        .text_size(px(text::META))
                                                        .text_color(hsla(c.you_dim))
                                                        .child(icon(
                                                            glyph,
                                                            px(12.),
                                                            hsla(c.you_dim),
                                                        ))
                                                        .child(SharedString::from(
                                                            a.lines()
                                                                .next()
                                                                .unwrap_or("")
                                                                .to_string(),
                                                        ))
                                                }),
                                            ))
                                        }),
                                    ix,
                                    c,
                                ),
                            )
                            .children(self.you_actions(ix, item, look)),
                    ),
                )
                .into_any_element(),
            Item::Agent { .. } => {
                let blocks = self.blocks_at(ix).unwrap_or_default();
                div()
                    .w_full()
                    .max_w(px(chat::COLUMN_MAX - 32.))
                    .text_color(hsla(c.text))
                    .when(depth > 0, |d| d.text_color(hsla(c.dim)))
                    .child(md_preview::column(&blocks, ix, &look.draw))
                    .into_any_element()
            }
            Item::Tool { .. } => self.tool(ix, item, look),
            // The todo list (spec B-B): a card with its count and a still meter, then the tasks.
            Item::Todo(list) => {
                let done = list.iter().filter(|t| t.state == TodoState::Done).count();
                let meter = crate::agent_top::meter_fill(
                    (done * 100).checked_div(list.len()).unwrap_or(0) as u8,
                    crate::agent_top::METER_W,
                );
                div()
                    .w_full()
                    .px(px(14.))
                    .py(px(10.))
                    .rounded(px(chat::R_CARD))
                    .bg(hsla(c.panel))
                    .border_1()
                    .border_color(hsla(c.panel_border))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(20.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(text::META))
                            .child(icon(Icon::ListChecks, px(12.), hsla(c.panel_dim)))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(hsla(c.panel_text))
                                    .child("Tasks"),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .font_features(crate::sidebar::tabular())
                                    .text_color(hsla(c.panel_dim))
                                    .child(SharedString::from(format!("{done} of {}", list.len()))),
                            )
                            .child(
                                div()
                                    .ml(px(2.))
                                    .relative()
                                    .w(px(crate::agent_top::METER_W))
                                    .h(px(crate::agent_top::METER_H))
                                    .rounded(px(2.))
                                    .bg(hsla(c.todo_track))
                                    .child(
                                        div()
                                            .absolute()
                                            .left_0()
                                            .top_0()
                                            .h_full()
                                            .w(px(meter))
                                            .rounded(px(2.))
                                            .bg(hsla(c.tool_ok)),
                                    ),
                            ),
                    )
                    .child(div().my(px(8.)).h(px(1.)).bg(hsla(c.tool_divider)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .text_size(px(13.))
                            .line_height(relative(1.45))
                            .children(list.iter().map(|t| {
                                let (glyph, ink, words, medium) = match t.state {
                                    TodoState::Done => (Icon::Check, c.tool_ok, c.panel_dim, false),
                                    TodoState::Active => {
                                        (Icon::CircleDot, c.todo_active, c.panel_text, true)
                                    }
                                    TodoState::Pending => {
                                        (Icon::Circle, c.panel_dim, c.panel_text, false)
                                    }
                                };
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(14.))
                                            .h(px(13. * 1.45))
                                            .flex()
                                            .items_center()
                                            .child(icon(glyph, px(12.), hsla(ink))),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.))
                                            .text_color(hsla(words))
                                            .when(medium, |d| d.font_weight(FontWeight::MEDIUM))
                                            .child(SharedString::from(t.text.clone())),
                                    )
                            })),
                    )
                    .into_any_element()
            }
            Item::Permission {
                tool,
                what,
                detail,
                always,
                answer,
                by,
                ..
            } => {
                match answer {
                    // Answered (spec B-B): one quiet line, a check or a cross before it.
                    Some(answer) => {
                        let allowed = matches!(answer, Answer::Allowed | Answer::AlwaysAllowed);
                        let (glyph, ink) = if allowed {
                            (Icon::Check, c.tool_ok)
                        } else {
                            (Icon::Stop, c.failed)
                        };
                        div()
                            .w_full()
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(text::META))
                            .text_color(hsla(c.dim))
                            .child(icon(glyph, px(12.), hsla(ink)))
                            .child(div().flex_1().min_w(px(0.)).truncate().child(
                                SharedString::from(format!(
                                    "{} · {tool} {what}",
                                    answer_line(answer)
                                )),
                            ))
                            .into_any_element()
                    }
                    None => self.permission_card(
                        tool,
                        what,
                        detail,
                        always.as_deref(),
                        by.as_deref(),
                        look,
                    ),
                }
            }
            // The agent's question (batch 3, chore 8): its card while open, else its summary.
            Item::Question {
                id,
                questions,
                reply,
                ..
            } => self.question_row(ix, id, questions, reply.as_ref(), look),
            // A fork's first row (batch 3, chore 2).
            Item::Notice(t) if self.fork_row(ix, t, look).is_some() => self
                .fork_row(ix, t, look)
                .unwrap_or_else(|| div().into_any_element()),
            // A notice (spec B-B): an info mark and the words, quiet, on the text's column.
            Item::Notice(t) => div()
                .w_full()
                .flex()
                .gap(px(6.))
                .text_size(px(text::META))
                .text_color(hsla(c.dim))
                .child(
                    div()
                        .flex_none()
                        .h(px(text::META * 1.5))
                        .flex()
                        .items_center()
                        .child(icon(Icon::Info, px(12.), hsla(c.dim))),
                )
                .child(match self.notice_action(ix, t, look) {
                    // "Stopped · Rewind to edit your message", "Rewound · Undo" (batch 3, chore 2).
                    Some(action) => div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(4.))
                        .child(SharedString::from(t.clone()))
                        .child(action),
                    None => div()
                        .flex_1()
                        .min_w(px(0.))
                        .child(SharedString::from(t.clone())),
                })
                .into_any_element(),
            Item::Error(t) => {
                let classic = self.provider.is_some() && self.conversation_id().is_some();
                let closable = self.provider.is_none();
                // An error (spec B-B): a card washed with the failure's colour, its octagon,
                // the message in the body's colour (readable), its buttons under it.
                div()
                    .w_full()
                    .px(px(14.))
                    .py(px(12.))
                    .rounded(px(chat::R_CARD))
                    .bg(hsla(c.error_fill))
                    .border_1()
                    .border_color(hsla(c.error_edge))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_none()
                                    .h(px(chat::BODY * chat::BODY_LINE))
                                    .flex()
                                    .items_center()
                                    .child(icon(Icon::Octagon, px(14.), hsla(c.error_mark))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_color(hsla(c.error_text))
                                    .child(SharedString::from(t.clone())),
                            ),
                    )
                    .when(classic || closable, |d| {
                        d.child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .text_size(px(text::META + 0.5))
                                .when(classic, |d| {
                                    d.child(
                                        button(
                                            ("agent-classic", ix),
                                            "Open in classic view",
                                            false,
                                            c,
                                        )
                                        .on_click(on_tab(&look.view, |this, cx| {
                                            this.open_classic(cx)
                                        })),
                                    )
                                })
                                .when(closable, |d| {
                                    d.child(
                                        button(("agent-close", ix), "Close", false, c).on_click(
                                            on_tab(&look.view, |_, cx| {
                                                cx.emit(AgentTabEvent::Close)
                                            }),
                                        ),
                                    )
                                }),
                        )
                    })
                    .into_any_element()
            }
        }
    }

    /// A tool call's row (spec B-B): its status (an arc while it runs, a dot once done), name,
    /// target and result, and a chevron when it has more to show; a run of tool rows draws as one
    /// group (`agent_chat::group_shape`). A click opens its detail in the modal (spec A).
    fn tool(&self, ix: usize, item: &Item, look: &Look) -> AnyElement {
        use crate::agent_chat::{Shape, chat, group_shape, summary_shown};
        let Item::Tool {
            id,
            name,
            summary,
            running,
            ok,
            result,
            detail,
            output,
            ..
        } = item
        else {
            return div().into_any_element();
        };
        let c = &look.c;
        let items = self.conversation.items();
        // A subagent's row (M11ar) reads from its lane: its name, its task, its state.
        let lane = self.conversation.lane(id).cloned();
        let steps = items
            .iter()
            .filter(|i| parent_of(i) == Some(id.as_str()))
            .count();
        let diff = match detail {
            Some(ToolDetail::Diff { unified, .. }) => Some(parse_diff(unified)),
            _ => None,
        };
        let openable = lane.is_some() || crate::agent_tool_modal::openable(items, ix);
        let (name, summary) = match &lane {
            Some(l) => (&l.label, &l.task.lines().next().unwrap_or("").to_string()),
            None => (name, summary),
        };
        let right: (String, Rgb) = if let Some(l) = &lane {
            // While it runs its last tool, then its step count; and its time when known.
            let what = if l.status.live() {
                l.last_tool.clone().unwrap_or_default()
            } else {
                crate::agent_lanes::steps_label(self.conversation.lane_steps(id))
            };
            let time = self.lane_elapsed(id).map(crate::agent_lanes::clock);
            let words = [Some(what), time]
                .into_iter()
                .flatten()
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            let ink = match l.status {
                crate::agent_conversation::LaneStatus::Failed(_) => c.failed_panel,
                _ => c.panel_dim,
            };
            (words, ink)
        } else if *running {
            let since = self
                .tool_started
                .get(id)
                .map_or(Duration::ZERO, |t| look.now.saturating_duration_since(*t));
            (elapsed_label(since), c.panel_dim)
        } else if *ok == Some(false) {
            let bare = result.trim_start_matches('✗').trim_start();
            (bare.to_string(), c.failed_panel)
        } else if let Some(d) = &diff {
            let added = d.rows.iter().filter(|r| r.kind == RowKind::Added).count();
            let removed = d.rows.iter().filter(|r| r.kind == RowKind::Removed).count();
            (format!("+{added} −{removed}"), c.panel_dim)
        } else if let (true, Some(out)) = (name == "Read", output) {
            (lines_label(out.lines().count()), c.panel_dim)
        } else {
            (result.clone(), c.panel_dim)
        };
        // Its status: running, the Working arc; ok, a dot; failed, a red dot; else a ring.
        let dot = |fill: Rgb| {
            div()
                .size(px(chat::TOOL_DOT))
                .rounded_full()
                .bg(hsla(fill))
                .into_any_element()
        };
        let status: AnyElement = match (&lane, running, ok) {
            (Some(l), _, _) => self.lane_mark(l, c),
            (None, true, _) => match self.tool_marks.get(id) {
                Some(mark) => mark.clone().into_any_element(),
                None => dot(c.todo_active),
            },
            (None, false, Some(true)) => dot(c.tool_ok),
            (None, false, Some(false)) => dot(c.failed_panel),
            (None, false, None) => div()
                .size(px(chat::TOOL_DOT))
                .rounded_full()
                .border_1()
                .border_color(hsla(c.panel_dim))
                .into_any_element(),
        };
        let chevron_slot = div()
            .flex_none()
            .size(px(crate::right_sidebar::CHEVRON))
            .when(openable, |d| {
                let (rest, hover) = (hsla(c.panel_dim), hsla(c.panel_text));
                d.child(
                    icon(Icon::ChevronRight, px(crate::right_sidebar::CHEVRON), rest)
                        .group_hover("agent-tool-row", move |s| s.text_color(hover)),
                )
            });
        let order = &self.pane().order;
        let shape = order
            .iter()
            .position(|&i| i == ix)
            .and_then(|k| crate::agent_chat::group_shape_at(items, order, k))
            .or_else(|| group_shape(items, ix))
            .unwrap_or(Shape::Solo);
        let (top, bottom) = match shape {
            Shape::Solo => (true, true),
            Shape::First => (true, false),
            Shape::Middle => (false, false),
            Shape::Last => (false, true),
        };
        let row_hover = hsla(c.row_hover);
        let tool = div()
            .id(("agent-tool", ix))
            .group("agent-tool-row")
            .relative()
            .w_full()
            .h(px(chat::TOOL_ROW))
            .px(px(chat::TOOL_PAD_X))
            .flex()
            .items_center()
            .gap(px(chat::TOOL_GAP))
            .bg(hsla(c.panel))
            .border_color(hsla(c.panel_border))
            .border_l_1()
            .border_r_1()
            .when(top, |d| d.border_t_1().rounded_t(px(chat::R_GROUP)))
            .when(bottom, |d| d.border_b_1().rounded_b(px(chat::R_GROUP)))
            .text_size(px(text::META))
            .child(
                div()
                    .flex_none()
                    .size(px(chat::TOOL_STATUS))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(status),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(hsla(c.tool_name))
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(name.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_color(hsla(c.panel_text))
                    .when(summary_shown(name, summary), |d| {
                        d.child(SharedString::from(summary.clone()))
                    }),
            )
            .when(steps > 0 && lane.is_none(), |d| {
                d.child(
                    div()
                        .flex_none()
                        .text_color(hsla(c.panel_dim))
                        .child(SharedString::from(format!("{steps} steps"))),
                )
            })
            .child(
                div()
                    .flex_none()
                    .max_w(relative(0.4))
                    .truncate()
                    .font_features(crate::sidebar::tabular())
                    .text_color(hsla(right.1))
                    .child(SharedString::from(right.0)),
            )
            .child(chevron_slot)
            // The line between a group's rows, from the name's column (spec B-B).
            .when(!bottom, |d| {
                d.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left(px(chat::TOOL_DIVIDER_INSET))
                        .right_0()
                        .h(px(1.))
                        .bg(hsla(c.tool_divider)),
                )
            });
        // A subagent's row opens its lane (M11ar).
        if lane.is_some() {
            let view = look.view.clone();
            let to = crate::agent_lanes::View::Lane(id.clone());
            return tool
                .cursor_pointer()
                .hover(move |s| s.bg(row_hover))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    let to = to.clone();
                    view.update(cx, |this, cx| this.show_view(to, cx)).ok();
                })
                .into_any_element();
        }
        // The whole row opens the tool's detail (spec A §A): a modal, never inline.
        let tool = if openable {
            let view = look.view.clone();
            let id = id.clone();
            tool.cursor_pointer()
                .hover(move |s| s.bg(row_hover))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    let id = id.clone();
                    view.update(cx, |this, cx| this.open_tool(id, cx)).ok();
                })
        } else {
            tool
        };
        tool.into_any_element()
    }

    /// An edit's diff, coloured as the file tab colours its diff rows.
    pub(super) fn diff_block(&self, d: &crate::diff_view::Parsed, look: &Look) -> AnyElement {
        let c = &look.c;
        let lines: Vec<&str> = d.text.split('\n').collect();
        div()
            .w_full()
            .py(px(4.))
            .rounded(px(5.))
            .bg(hsla(c.page))
            .font_family(look.mono.clone())
            .text_size(px(BODY - 1.5))
            .flex()
            .flex_col()
            .children(d.rows.iter().zip(lines).map(|(row, line)| {
                let (fill, sign, ink) = match row.kind {
                    RowKind::Added => (Some(look.diff.added_fill), "+", look.diff.added_ink),
                    RowKind::Removed => (Some(look.diff.removed_fill), "−", look.diff.removed_ink),
                    RowKind::Same | RowKind::Gap => (None, " ", c.dim),
                };
                div()
                    .flex()
                    .px(px(8.))
                    .when_some(fill, |d, f| d.bg(hsla(f)))
                    .child(
                        div()
                            .flex_none()
                            .w(px(14.))
                            .text_color(hsla(ink))
                            .child(sign),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_color(hsla(c.text))
                            .child(SharedString::from(line.to_string())),
                    )
            }))
            .into_any_element()
    }

    /// The open permission card: what the agent wants to run, and Allow / always / Deny.
    fn permission_card(
        &self,
        tool: &str,
        what: &str,
        detail: &str,
        always: Option<&str>,
        by: Option<&str>,
        look: &Look,
    ) -> AnyElement {
        let c = &look.c;
        let mut shown: Vec<&str> = detail.lines().take(DETAIL_LINES).collect();
        if detail.lines().count() > DETAIL_LINES {
            shown.push("…");
        }
        // A subagent's request names it (M11ar): "Reader wants to run Bash".
        let label: SharedString = match by.and_then(|b| self.conversation.lane(b)) {
            Some(lane) => lane.label.clone().into(),
            None => look.label.clone(),
        };
        // Spec B-B: a card (r12), a shield before its heading, the command in a well.
        div()
            .w_full()
            .px(px(14.))
            .py(px(12.))
            .rounded(px(crate::agent_chat::chat::R_CARD))
            .border_1()
            .border_color(hsla(c.ask_edge))
            .bg(hsla(c.ask_fill))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(hsla(c.ask_head))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(icon(Icon::Shield, px(14.), hsla(c.ask_head)))
                    .child(SharedString::from(format!("{label} wants to run {tool}"))),
            )
            .child(
                div()
                    .w_full()
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(crate::agent_chat::chat::R_GROUP))
                    .bg(hsla(c.code_fill))
                    .font_family(look.mono.clone())
                    .text_size(px(12.5))
                    .text_color(hsla(c.code_text))
                    .child(SharedString::from(what.to_string())),
            )
            .when(!detail.trim().is_empty(), |d| {
                d.child(
                    div()
                        .font_family(look.mono.clone())
                        .text_size(px(text::META))
                        .text_color(hsla(c.ask_dim))
                        .child(SharedString::from(shown.join("\n"))),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .text_size(px(text::META + 0.5))
                    .child(button("agent-allow", "Allow  ⏎", true, c).on_click(on_tab(
                        &look.view,
                        |this, cx| {
                            this.answer(Answer::Allowed, cx).ok();
                        },
                    )))
                    .when_some(always.map(str::to_string), |d, always| {
                        d.child(button("agent-always", always, false, c).on_click(on_tab(
                            &look.view,
                            |this, cx| {
                                this.answer(Answer::AlwaysAllowed, cx).ok();
                            },
                        )))
                    })
                    .child(button("agent-deny", "Deny", false, c).on_click(on_tab(
                        &look.view,
                        |this, cx| {
                            this.answer(Answer::Denied(None), cx).ok();
                        },
                    ))),
            )
            .child(
                div()
                    .text_size(px(text::META))
                    .text_color(hsla(c.ask_dim))
                    .child(SharedString::from(format!(
                        "Or type in the box below to tell {label} what to do instead."
                    ))),
            )
            .into_any_element()
    }

    /// The right button: Stop while the turn runs, "Stopping…" until it ends, then Force stop.
    pub(crate) fn stop_label(&self) -> Option<&'static str> {
        match (self.stopping.is_some(), self.force) {
            (true, true) => Some("Force stop"),
            (true, false) => Some("Stopping…"),
            (false, _) if self.status() == Status::Working => Some("■ Stop  esc"),
            _ => None,
        }
    }

    /// Your message's text, its pasted images' markers drawn as pills (the brackets clear, the
    /// name on the chip fill) that open the image when clicked.
    fn you_text(&self, ix: usize, text: &str, look: &Look) -> AnyElement {
        let c = &look.c;
        let marks = marker_ranges(text, self.images.keys());
        if marks.is_empty() {
            let shown = SharedString::from(text.to_string());
            let styled = gpui::StyledText::new(shown.clone());
            let layout = styled.layout().clone();
            return div()
                .child(selectable(look, shown, layout, styled.into_any_element()))
                .into_any_element();
        }
        // Each marker on the chip fill, its brackets drawn as spaces (the pill's padding; the
        // same length, so the ranges hold).
        let pill = gpui::HighlightStyle {
            color: Some(hsla(c.you_text)),
            background_color: Some(hsla(c.chip_fill)),
            ..Default::default()
        };
        let highlights: Vec<(std::ops::Range<usize>, gpui::HighlightStyle)> =
            marks.iter().map(|(r, _)| (r.clone(), pill)).collect();
        let mut shown = text.to_string();
        for (r, _) in &marks {
            shown.replace_range(r.start..r.start + 1, " ");
            shown.replace_range(r.end - 1..r.end, " ");
        }
        let names: Vec<String> = marks.iter().map(|(_, n)| n.clone()).collect();
        let view = look.view.clone();
        let shown = SharedString::from(shown);
        let styled = gpui::StyledText::new(shown.clone()).with_highlights(highlights);
        let layout = styled.layout().clone();
        let pills = gpui::InteractiveText::new(("you-text", ix), styled)
            .on_click(
                marks.into_iter().map(|(r, _)| r).collect(),
                move |i, _, cx| {
                    if let Some(name) = names.get(i).cloned() {
                        view.update(cx, |this, cx| this.view_image(&name, cx)).ok();
                    }
                },
            )
            .into_any_element();
        selectable(look, shown, layout, pills)
    }
}

/// Your message's text made selectable (chore after M11ar), as the agent's is in `md_preview`.
fn selectable(
    look: &Look,
    text: SharedString,
    layout: gpui::TextLayout,
    child: AnyElement,
) -> AnyElement {
    match &look.draw.select {
        Some((state, wash)) => {
            crate::chat_select::SelectableText::new(state.clone(), text, layout, child, *wash)
                .into_any_element()
        }
        None => child,
    }
}
