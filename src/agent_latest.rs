//! The chat's latest-prompt strip (chore after M11ar; your words: "on chatbox add the floating
//! latest chat on top, when clicked, it goes to that chat … so that its easy to know what I
//! prompt when the agent is running"): one line of your latest message pinned over the chat's
//! top edge while that message is scrolled above the view (a lane's view: the task Main gave
//! it). A click glides the chat back to it and briefly lights it.
//!
//! Cheap per frame: the message's row and its line are kept by `sync_rows` (the conversation
//! keeps your latest's index as items come); a draw compares that row with the list's scroll
//! top. The decision is made before the list lays out, so a canvas after the list checks it
//! again with this frame's scroll top and asks for one more frame when they differ (output
//! pushing your message off while the chat follows its end).

use super::*;
use crate::agent_chat::{FLASH_MS, GLIDE_MS, chat, flash_strength, glide_at, glide_from};
use crate::agent_lanes::View;
use gpui::{ClickEvent, Div, ListOffset, canvas};

/// The strip: a tool row's height, set in from the chat's top by its first row's gap; what of
/// a message is under it once it is scrolled that far (it counts as off screen).
const STRIP_H: f32 = chat::TOOL_ROW;
const STRIP_TOP: f32 = chat::FIRST_TOP;
const STRIP_COVER: f32 = STRIP_TOP + STRIP_H;
/// The page's band behind it fades out this far under it.
const BAND_FADE: f32 = 12.0;
/// Its fade in as it appears (none with reduce motion: GPUI draws an animation still then).
const FADE_MS: u64 = 120;
pub(crate) const STRIP_TIP: &str = "Jump to your message";

/// The message a view's strip says: its row in the view's list, and its one line.
pub(super) struct Latest {
    row: usize,
    key: u64,
    line: SharedString,
}

impl Latest {
    /// The strip's message after the rows changed: `said` is its row and text (none: no strip).
    /// The line is built again only when the text changed.
    pub(super) fn update(old: Option<Latest>, said: Option<(usize, &str)>) -> Option<Latest> {
        let (row, text) = said?;
        let mut h = DefaultHasher::new();
        text.hash(&mut h);
        let key = h.finish();
        match old {
            Some(l) if l.key == key => Some(Latest { row, ..l }),
            _ => {
                let line = crate::agent_chat::strip_line(text);
                (!line.is_empty()).then(|| Latest {
                    row,
                    key,
                    line: line.into(),
                })
            }
        }
    }
}

/// The strip's state in a tab: whether the last draw showed it (and how many times it has
/// appeared: its fade's key), the jump's glide and the jumped-to message's highlight.
#[derive(Default)]
pub(super) struct Strip {
    shown: bool,
    appeared: u64,
    glide: Option<Glide>,
    flash: Option<Flash>,
    flashes: u64,
}

/// The jump gliding up to `row` (scroll offsets in points), started `at`.
struct Glide {
    view: View,
    row: usize,
    from: f32,
    to: f32,
    at: Instant,
}

/// The jumped-to message, lit for `FLASH_MS`: its view and row, and its animation's key.
struct Flash {
    view: View,
    row: usize,
    key: u64,
    at: Instant,
}

/// How far the list is scrolled, in points.
fn scrolled(list: &ListState) -> f32 {
    -f32::from(list.scroll_px_offset_for_scrollbar().y)
}

/// Whether the strip shows for the message at `row`, from the list as it was last laid out.
fn strip_visible(list: &ListState, row: usize) -> bool {
    let laid_out = list.viewport_bounds().size.height > px(0.);
    let top = laid_out.then(|| {
        let t = list.logical_scroll_top();
        (t.item_ix, f32::from(t.offset_in_item))
    });
    // Its height matters only while it is the first row in sight.
    let height = match top {
        Some((ix, _)) if ix == row => list.bounds_for_item(row).map(|b| f32::from(b.size.height)),
        _ => None,
    };
    crate::agent_chat::strip_shows(Some(row), top, height, STRIP_COVER)
}

impl AgentTab {
    /// The strip's layer over the chat (none without a message): drawn when the message is
    /// above the view, and the check after the list's layout. Steps the jump's glide first.
    pub(super) fn latest_layer(
        &mut self,
        c: &Colors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.glide_step(window);
        if self
            .strip
            .flash
            .as_ref()
            .is_some_and(|f| f.at.elapsed() >= Duration::from_millis(FLASH_MS + GLIDE_MS))
        {
            // Over: rows drawn from now on carry no highlight (one scrolled back in would light
            // again: its animation's state went with it).
            self.strip.flash = None;
        }
        let pane = self.pane();
        let Some(latest) = &pane.latest else {
            self.strip.shown = false;
            return None;
        };
        let (row, line, list) = (latest.row, latest.line.clone(), pane.list.clone());
        // A short chat with the question docked: the strip makes room (batch 3, chore 8); and
        // it never covers an open ask's head (the design review).
        let room =
            !self.ask_hides_strip(f32::from(list.viewport_bounds().size.height), STRIP_COVER);
        let ask_row = self.ask_row();
        let visible = move |list: &ListState| {
            room && strip_visible(list, row)
                && !super::ask_view::ask_under_strip(list, ask_row, STRIP_COVER)
        };
        let shows = visible(&list);
        if shows && !self.strip.shown {
            self.strip.appeared += 1;
        }
        self.strip.shown = shows;
        let tab = cx.entity_id();
        let check = canvas(
            move |_, window, _| {
                if visible(&list) != shows {
                    window.on_next_frame(move |_, cx| cx.notify(tab));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0();
        let strip = shows.then(|| {
            // The page behind it, fading out under it, so no row shows as a sliver above it.
            let page = hsla(c.page);
            let band = div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(STRIP_COVER + BAND_FADE))
                .bg(gpui::linear_gradient(
                    180.,
                    gpui::linear_color_stop(page, STRIP_COVER / (STRIP_COVER + BAND_FADE) - 0.1),
                    gpui::linear_color_stop(gpui::Hsla { a: 0., ..page }, 1.),
                ));
            div()
                .w_full()
                .pt(px(STRIP_TOP))
                .px(px(AREA_X))
                .flex()
                .justify_center()
                .child(band)
                .child(self.strip_card(line, c, cx))
                .with_animation(
                    ("agent-latest-strip", self.strip.appeared),
                    Animation::new(Duration::from_millis(FADE_MS))
                        .with_easing(crate::agent_top::ease_out_cubic),
                    |d, t| d.opacity(t),
                )
        });
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .child(check)
                .children(strip)
                .into_any_element(),
        )
    }

    /// The strip itself: the panel's card, a tool row tall, its mark and your line.
    fn strip_card(&self, line: SharedString, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let dark = c.page_is_dark;
        let shadow = |y: f32, blur: f32, a: f32| gpui::BoxShadow {
            color: gpui::Hsla { a, ..gpui::black() },
            offset: gpui::point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(0.),
            inset: false,
        };
        let hover = c.panel_hover;
        let card = div()
            .id("agent-latest-strip")
            .w_full()
            .max_w(px(chat::COLUMN_MAX))
            .h(px(STRIP_H))
            .px(px(chat::TOOL_PAD_X))
            .flex()
            .items_center()
            .gap(px(chat::TOOL_GAP))
            .rounded(px(chat::R_GROUP))
            .bg(hsla(c.panel))
            .border_1()
            .border_color(hsla(c.panel_border))
            .shadow(vec![
                shadow(1., 2., if dark { 0.30 } else { 0.06 }),
                shadow(6., 16., if dark { 0.30 } else { 0.10 }),
            ])
            .text_size(px(chat::BODY))
            .text_color(hsla(c.strip_text))
            // Over the chat: a press here is the strip's, not a selection's; the wheel still
            // scrolls the chat under it.
            .block_mouse_except_scroll()
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .tooltip(|_, cx| {
                cx.new(|_| crate::sidebar_toggle::Tip(STRIP_TIP.into()))
                    .into()
            })
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.jump_to_latest(cx)))
            .child(icon(Icon::ArrowUp, px(12.), hsla(c.strip_mark)))
            .child(div().flex_1().min_w(px(0.)).truncate().child(line));
        self.marked("latest-strip", card).into_any_element()
    }

    /// The strip's click: the chat to its message, that message at the top of the view (gliding
    /// there unless motion is reduced), then lit a moment. Following the chat's end stops, as a
    /// scroll up stops it; scrolling back to the end resumes it.
    pub(super) fn jump_to_latest(&mut self, cx: &mut Context<Self>) {
        // A question waits (batch 3, chore 8): the click lands on its card.
        if self.asking() {
            self.reveal_ask(cx);
            return;
        }
        let pane = self.pane();
        let Some(row) = pane.latest.as_ref().map(|l| l.row) else {
            return;
        };
        let list = pane.list.clone();
        let from = scrolled(&list);
        list.scroll_to(ListOffset {
            item_ix: row,
            offset_in_item: px(0.),
        });
        self.strip.glide = (!cx.reduce_motion()).then(|| {
            let to = scrolled(&list);
            let screen = f32::from(list.viewport_bounds().size.height);
            Glide {
                view: self.view.clone(),
                row,
                from: glide_from(from, to, screen),
                to,
                at: Instant::now(),
            }
        });
        self.strip.flashes += 1;
        self.strip.flash = Some(Flash {
            view: self.view.clone(),
            row,
            key: self.strip.flashes,
            at: Instant::now(),
        });
        cx.notify();
    }

    /// One frame of the jump's glide (its first jumps to where the glide starts). Ends on the
    /// message's row exactly; another view shown ends it.
    fn glide_step(&mut self, window: &mut Window) {
        let Some(g) = &self.strip.glide else {
            return;
        };
        if g.view != self.view {
            self.strip.glide = None;
            return;
        }
        let list = self.pane().list.clone();
        let t = g.at.elapsed().as_secs_f32() * 1000. / GLIDE_MS as f32;
        if t >= 1. {
            list.scroll_to(ListOffset {
                item_ix: g.row,
                offset_in_item: px(0.),
            });
            self.strip.glide = None;
            return;
        }
        list.scroll_by(px(glide_at(g.from, g.to, t) - scrolled(&list)));
        window.request_animation_frame();
    }

    /// The highlight's key while row `row` of the shown view is the jumped-to message.
    fn flash_key(&self, row: usize) -> Option<u64> {
        self.strip
            .flash
            .as_ref()
            .filter(|f| f.view == self.view && f.row == row)
            .map(|f| f.key)
    }

    /// Your bubble for item `ix`, lit while it is the jumped-to message (Main's rows are its
    /// items, sorted: its row is found only while a highlight is on).
    pub(super) fn flashed_you(&self, bubble: Div, ix: usize, c: &Colors) -> AnyElement {
        let pane = self.pane();
        let row = match &self.strip.flash {
            Some(_) if !pane.head => pane.order.binary_search(&ix).ok(),
            _ => None,
        };
        match row {
            Some(row) => self.flashed(bubble, row, c),
            None => bubble.into_any_element(),
        }
    }

    /// A bubble at row `row`, lit while it is the jumped-to message: its fill and edge drawn
    /// towards the accent, holding, then fading back to rest.
    pub(super) fn flashed(&self, bubble: Div, row: usize, c: &Colors) -> AnyElement {
        let Some(key) = self.flash_key(row) else {
            return bubble.into_any_element();
        };
        let (fill, edge, accent) = (c.you_fill, c.you_edge, c.accent);
        bubble
            .with_animation(
                ("agent-you-flash", key),
                Animation::new(Duration::from_millis(FLASH_MS)),
                move |d, t| {
                    let k = flash_strength(t);
                    d.bg(hsla(mix(fill, accent, 0.18 * k)))
                        .border_color(hsla(mix(edge, accent, 0.9 * k)))
                },
            )
            .into_any_element()
    }

    /// The strip's line while it shows (scenes).
    #[cfg(feature = "ui-shots")]
    pub fn shot_latest_strip(&self) -> Option<String> {
        self.strip
            .shown
            .then(|| self.pane().latest.as_ref().map(|l| l.line.to_string()))
            .flatten()
    }

    /// The row the strip's message is at in the shown view (scenes).
    #[cfg(feature = "ui-shots")]
    pub fn shot_latest_row(&self) -> Option<usize> {
        self.pane().latest.as_ref().map(|l| l.row)
    }

    /// Whether the jump's glide still runs (scenes wait for it).
    #[cfg(feature = "ui-shots")]
    pub fn shot_gliding(&self) -> bool {
        self.strip.glide.is_some()
    }
}
