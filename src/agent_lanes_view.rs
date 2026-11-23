//! The agent tab's subagents (M11ar): following a subagent's transcript (agy writes each
//! subagent's steps to its own file), the pill strip that switches the chat between Main and
//! each subagent (a lane), and the keys that step through them.

use super::*;
use crate::agent_conversation::{Lane, LaneStatus};
use crate::agent_lanes::{Bar, LaneBox, View};

/// How soon after a draft's lane finished under it an Enter only flashes (the lanes chore: the
/// designer's 600 ms, as `FLASH_MS`).
pub(crate) const RETARGET_HOLD: Duration = Duration::from_millis(crate::agent_chat::FLASH_MS);
/// The closing bar's cross-fade with the box.
pub(crate) const BAR_FADE: Duration = Duration::from_millis(120);

/// The strip's height (the Lead UI/UX Designer's spec).
const STRIP_H: f32 = 32.0;
/// A tab narrower than this cuts labels shorter.
const NARROW: f32 = 480.0;

impl AgentTab {
    /// `Event::Follow`: read `path`'s new lines as lane `lane`'s (`agent_lanes::follow_every`:
    /// often while it works, rarely once done), off the UI thread, through the translator. It lives as long as the process (a woken subagent
    /// writes again after its report); a file that shrinks ends it.
    pub(super) fn follow(&mut self, lane: String, path: PathBuf, cx: &mut Context<Self>) {
        let task = cx.spawn({
            let lane = lane.clone();
            async move |this, cx| {
                let mut tail = crate::lane_follow::Tail::default();
                loop {
                    let Ok(every) = this.update(cx, |this, _| {
                        crate::agent_lanes::follow_every(this.conversation.lane(&lane))
                    }) else {
                        break;
                    };
                    cx.background_executor().timer(every).await;
                    let (at, file) = (tail.offset, path.clone());
                    let read = cx
                        .background_executor()
                        .spawn(async move {
                            crate::lane_follow::read_new(&file, at, crate::lane_follow::MAX_READ)
                        })
                        .await;
                    let Ok(bytes) = read else { break };
                    let lines = tail.feed(&bytes);
                    if lines.is_empty() {
                        continue;
                    }
                    let alive = this
                        .update(cx, |this, cx| this.on_followed(&lane, lines, cx))
                        .is_ok();
                    if !alive {
                        break;
                    }
                }
            }
        });
        self.followers.insert(lane, task);
    }

    /// A followed file's new lines for `lane`, through the translator, as events.
    pub(super) fn on_followed(&mut self, lane: &str, lines: Vec<String>, cx: &mut Context<Self>) {
        let Some(translator) = self.translator.as_mut() else {
            return;
        };
        let events: Vec<Event> = lines
            .iter()
            .flat_map(|l| translator.decode_followed(lane, l))
            .collect();
        if !events.is_empty() {
            self.on_events(events, cx);
        }
    }

    /// Show `view` (Main or a lane): its own list, kept as it was.
    pub(crate) fn show_view(&mut self, view: View, cx: &mut Context<Self>) {
        self.lanes_menu = false;
        if self.view != view {
            self.view = view;
            // Leaving a view closes its opened bar; the box's words are the new view's draft.
            self.bar_open = None;
            self.draft = None;
            self.retargeted_at = None;
            self.lane_box_changed(cx);
            self.bar_was = matches!(self.box_mode(cx), BoxMode::Bar(_));
            self.sync_rows();
            self.update_placeholder(cx);
        }
        cx.notify();
    }

    /// Main's process can take a message now (the lanes chore: a relay goes at once).
    pub(crate) fn lane_ready(&self, cx: &App) -> bool {
        (self.process.is_some() || self.no_process)
            && !self.booting
            && !self.ending
            && !self.paused
            && !self.restart_waits(cx)
    }

    /// An image waits in the box (it can't be passed on).
    pub(crate) fn has_images(&self) -> bool {
        self.attachments
            .iter()
            .any(|a| matches!(a, Attachment::Image { .. }))
    }

    /// What the box is now (the lanes chore): `agent_lanes::lane_box` for the shown lane, the
    /// bar opened into the box, and the draft's target (a lane finishing mid-draft never swaps
    /// the box). An open question or permission takes the box first.
    pub(crate) fn box_mode(&self, cx: &App) -> BoxMode {
        let Some(lane) = self.shown_lane() else {
            return BoxMode::Main;
        };
        if self.conversation.oldest_ask().is_some() {
            return BoxMode::Main;
        }
        let relays = self.provider.is_some_and(|p| p.relays_to_subagents());
        let ready = self.lane_ready(cx);
        let draft = self.draft.as_ref().filter(|d| d.lane == lane.id);
        match crate::agent_lanes::lane_box(lane, relays, ready, self.has_images()) {
            LaneBox::ToMain => BoxMode::MainFromLane,
            LaneBox::Relay { .. } if draft.is_some_and(|d| d.about) => {
                BoxMode::About { live: true }
            }
            LaneBox::Relay { images } => BoxMode::Relay {
                images,
                ready: true,
            },
            // A relay half-typed while Main gets ready: kept, its send off.
            LaneBox::Bar(Bar::NotReady) if draft.is_some_and(|d| !d.about) => BoxMode::Relay {
                images: self.has_images(),
                ready: false,
            },
            LaneBox::Bar(why) => {
                if self.bar_open.as_deref() == Some(lane.id.as_str()) || draft.is_some() {
                    BoxMode::About {
                        live: !matches!(why, Bar::Finished(_)),
                    }
                } else {
                    BoxMode::Bar(why)
                }
            }
        }
    }

    /// Enter in a lane's box (the lanes chore): `None` when the box talks to Main as ever; else
    /// whether it sent (the box is cleared). You can never message a finished subagent: a relay
    /// goes only while `lane_box` says so; else Enter sends the "about" message to Main, goes
    /// Back to Main (the bar), or does nothing.
    pub(super) fn lane_submit(&mut self, text: &str, cx: &mut Context<Self>) -> Option<bool> {
        let lane = self.shown_lane()?.clone();
        if self.paused {
            return None;
        }
        let empty = text.trim().is_empty() && self.attachments.is_empty();
        match self.box_mode(cx) {
            BoxMode::Main | BoxMode::MainFromLane => None,
            BoxMode::Bar(_) => {
                if empty {
                    self.back_to_main(&lane.id, cx);
                }
                Some(false)
            }
            BoxMode::Relay { images, ready } => {
                if empty || images || !ready {
                    return Some(false);
                }
                let to = Target::of(&lane, false, true);
                self.send_for_lane(text.to_string(), to, cx);
                Some(true)
            }
            BoxMode::About { live } => {
                // Just retargeted under your fingers: a flash, nothing sent.
                if self
                    .retargeted_at
                    .is_some_and(|at| at.elapsed() < RETARGET_HOLD)
                {
                    self.lane_flash.insert(lane.id.clone(), Instant::now());
                    cx.notify();
                    return Some(false);
                }
                if empty {
                    return Some(false);
                }
                let to = Target::of(&lane, true, live);
                self.send_for_lane(text.to_string(), to, cx);
                // Sent to Main: shown there.
                self.show_view(View::Main, cx);
                Some(true)
            }
        }
    }

    /// Send `text` for subagent `to` (a relay, or about it): through Main, as a message of
    /// yours (held like one while the process isn't there).
    fn send_for_lane(&mut self, text: String, to: Target, cx: &mut Context<Self>) {
        let attachments = std::mem::take(&mut self.attachments);
        self.suggestion = None;
        let folder = self.history_folder();
        let write = !self.no_process;
        crate::prompt_history::PromptHistory::record(&folder, &text, write, cx);
        self.recall = None;
        self.draft = None;
        self.retargeted_at = None;
        self.bar_open = None;
        if self.booting || self.ending || self.restart_waits(cx) {
            self.queued.push((text, attachments, Some(to)));
            cx.notify();
            return;
        }
        if self.process.is_none() && !self.no_process {
            let resume = self.conversation_id();
            self.spawn_process(resume, cx);
        }
        self.flush_queued();
        self.say_to(text, attachments, Some(to));
        self.after_change(true, cx);
    }

    /// The box's words changed (the lanes chore): typing (a paste, ↑) in the closing bar opens
    /// it into the box, the letters carried; the draft's target is set when the box stops
    /// being empty, and dropped when it empties.
    pub(super) fn lane_box_changed(&mut self, cx: &mut Context<Self>) {
        let empty = self.composer.read(cx).text().is_empty();
        let Some(lane) = self.shown_lane().map(|l| l.id.clone()) else {
            self.draft = None;
            return;
        };
        if empty {
            if self.draft.take().is_some() {
                self.retargeted_at = None;
                self.update_placeholder(cx);
            }
            return;
        }
        if self.draft.is_some() {
            return;
        }
        let about = match self.box_mode(cx) {
            BoxMode::Relay { .. } => false,
            BoxMode::About { .. } => true,
            BoxMode::Bar(_) => {
                self.bar_open = Some(lane.clone());
                true
            }
            BoxMode::Main | BoxMode::MainFromLane => return,
        };
        self.draft = Some(Draft { lane, about });
        self.note_bar(cx);
        self.update_placeholder(cx);
    }

    /// The box's placeholder in a lane (the lanes chore); none in Main's view or while an ask
    /// takes the box.
    pub(super) fn lane_placeholder(&self, cx: &App) -> Option<String> {
        let label = self.shown_lane()?.label.clone();
        Some(match self.box_mode(cx) {
            BoxMode::Main => return None,
            // agy: the box still talks to Main (M11ar).
            BoxMode::MainFromLane => format!("Message Main \u{2014} it can pass this to {label}"),
            BoxMode::Relay { .. } if self.status() == Status::Working => {
                format!("Message {label} \u{2014} Main passes it on after its step")
            }
            BoxMode::Relay { .. } => format!("Message {label} \u{2014} Main passes it on"),
            BoxMode::About { live: false } => format!("Ask Main about {label}'s result\u{2026}"),
            BoxMode::About { live: true } => format!("Ask Main about {label}\u{2026}"),
            BoxMode::Bar(_) => String::new(),
        })
    }

    /// The box's target pill (the lanes chore), first in its folder tab: the lane's own mark,
    /// "To Explore · via Main" (a relay) or "To Main · about Explore"; no hover, no click. It
    /// flashes when the lane changes under a draft (or a relay is delivered).
    pub(crate) fn target_pill(&self, mode: &BoxMode, c: &Colors, cx: &App) -> Option<AnyElement> {
        let lane = self.shown_lane()?;
        let narrow = self.bar_width.get() < NARROW;
        let label = lane.label.clone();
        let (strong, quiet, tip) = match mode {
            BoxMode::Relay { .. } => (
                format!("To {label}"),
                (!narrow).then_some(" \u{b7} via Main".to_string()),
                Some(format!(
                    "Main passes your message to {label}. It reads it after its current step."
                )),
            ),
            BoxMode::About { .. } => (
                "To Main".to_string(),
                Some(format!(" \u{b7} about {label}")),
                None,
            ),
            _ => return None,
        };
        let (fill, accent) = (c.relay_fill, c.accent);
        let pill = div()
            .id("relay-target")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(5.))
            .h(px(crate::card_view::PILL_H))
            .px(px(8.))
            .rounded(px(crate::card_view::PILL_H / 2.))
            .bg(hsla(fill))
            .text_size(px(12.))
            .child(self.lane_mark(lane, c))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(c.relay_text))
                            .child(SharedString::from(strong)),
                    )
                    .children(quiet.map(|q| {
                        div()
                            .text_color(hsla(c.relay_dim))
                            .child(SharedString::from(q))
                    })),
            )
            .when_some(tip, |d, tip| {
                d.tooltip(move |_, cx| {
                    cx.new(|_| crate::sidebar_toggle::Tip(tip.clone().into()))
                        .into()
                })
            });
        let pill = self.marked("relay-target", pill);
        Some(match self.lane_flash_key(&lane.id, cx) {
            Some(key) => pill
                .with_animation(
                    ("relay-target-flash", key),
                    Animation::new(Duration::from_millis(crate::agent_chat::FLASH_MS)),
                    move |d, t| {
                        let k = crate::agent_chat::flash_strength(t);
                        d.bg(hsla(mix(fill, accent, 0.35 * k)))
                    },
                )
                .into_any_element(),
            None => pill.into_any_element(),
        })
    }

    /// The closing bar's two rows (the lanes chore), in the box's own geometry: row 1 at the
    /// typing's line height (the lane's mark and what became of it, in the UI font; the box's
    /// text is under it, unseen, so typing opens the box with its letters), row 2 at the
    /// buttons' (Ask Main about it, or start typing, Back to Main ⏎).
    pub(crate) fn bar_rows(
        &self,
        why: &Bar,
        c: &Colors,
        line_height: gpui::Pixels,
        inset: (f32, f32, f32),
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, AnyElement)> {
        let lane = self.shown_lane()?;
        let narrow = self.bar_width.get() < NARROW;
        let (text_x, controls_x, notch_room) = inset;
        let name = lane.label.clone();
        let time = self.lane_elapsed(&lane.id).map(crate::agent_lanes::clock);
        let ring = |ink: Rgb| {
            div()
                .size(px(crate::ui_tokens::chat::TOOL_DOT))
                .rounded_full()
                .border_1()
                .border_color(hsla(ink))
                .into_any_element()
        };
        // (mark, the strong words and their ink, the quiet rest, a tooltip for the cut reason)
        let (mark, strong, ink, rest, tip): (
            AnyElement,
            String,
            Rgb,
            Option<String>,
            Option<String>,
        ) = match why {
            Bar::Finished(LaneStatus::Done) => (
                crate::status_mark::done_check(c.bar_ok, px(MARK)).into_any_element(),
                format!("{name} finished"),
                c.bar_text,
                time.map(|t| format!(" in {t}")),
                None,
            ),
            Bar::Finished(LaneStatus::Failed(reason)) => (
                div()
                    .size(px(crate::ui_tokens::chat::TOOL_DOT))
                    .rounded_full()
                    .bg(hsla(c.bar_failed))
                    .into_any_element(),
                format!("{name} failed"),
                c.bar_failed,
                (!reason.trim().is_empty()).then(|| format!(" \u{b7} {}", reason.trim())),
                (!reason.trim().is_empty()).then(|| reason.trim().to_string()),
            ),
            Bar::Finished(LaneStatus::Stopped) => (
                ring(c.bar_dim),
                format!("{name} was stopped"),
                c.bar_text,
                time.map(|t| format!(" after {t}")),
                None,
            ),
            Bar::Finished(_) => (
                ring(c.bar_dim),
                format!("{name} ended"),
                c.bar_text,
                Some(" without a report".to_string()),
                None,
            ),
            Bar::Foreground => (
                self.lane_mark(lane, c),
                format!("{name} is working inside Main's step"),
                c.bar_text,
                Some(" \u{b7} messages reach it only in the background".to_string()),
                None,
            ),
            Bar::NotReady => (
                self.lane_mark(lane, c),
                format!("{name} is working"),
                c.bar_text,
                Some(" \u{b7} you can message it once Main is ready".to_string()),
                None,
            ),
        };
        let words = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(text::UI))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(ink))
                    .child(SharedString::from(strong)),
            )
            .children(rest.map(|r| {
                div()
                    .min_w(px(0.))
                    .truncate()
                    .font_features(crate::sidebar::tabular())
                    .text_color(hsla(c.bar_dim))
                    .child(SharedString::from(r))
            }));
        let words = match tip {
            Some(tip) => div()
                .id("bar-words")
                .flex_1()
                .min_w(px(0.))
                .flex()
                .child(words)
                .tooltip(move |_, cx| {
                    cx.new(|_| crate::sidebar_toggle::Tip(tip.clone().into()))
                        .into()
                })
                .into_any_element(),
            None => words.into_any_element(),
        };
        let row1 = div()
            .relative()
            .w_full()
            .h(line_height)
            .pl(px(text_x))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(mark)
            .child(words)
            // The box's text, unseen (its caret hidden): typing, a paste or ↑ opens the box.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .opacity(0.)
                    .child(self.composer.clone()),
            );
        let row1 = self.marked("closing-bar", row1).into_any_element();
        let hover = c.control_hover;
        let ask = self.marked(
            "bar-ask",
            div()
                .id("bar-ask")
                .flex_none()
                .h(px(28.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(14.))
                .border_1()
                .border_color(hsla(c.outer_edge))
                .text_size(px(text::META))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(c.bar_text))
                .cursor_pointer()
                .hover(move |s| s.bg(hsla(hover)))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_bar(cx);
                    let focus = this.composer.read(cx).focus_handle(cx);
                    window.focus(&focus, cx);
                }))
                .child("Ask Main about it"),
        );
        let row2 = div()
            .w_full()
            .h(px(28.))
            .pl(px(controls_x))
            .flex()
            .items_center()
            .child(ask)
            .when(!narrow, |d| {
                d.child(
                    div()
                        .flex_none()
                        .ml(px(8.))
                        .text_size(px(text::META))
                        .text_color(hsla(c.bar_dim))
                        .child("or start typing"),
                )
            })
            .child(div().flex_1())
            .when(!narrow, |d| {
                d.child(
                    div()
                        .flex_none()
                        .text_size(px(text::KEYS))
                        .text_color(hsla(c.bar_dim))
                        .child("Back to Main \u{23ce}"),
                )
            })
            .child(div().flex_none().w(px(notch_room)))
            .into_any_element();
        Some((row1, row2))
    }

    /// The chip leading your message in Main when it was for a subagent (the lanes chore):
    /// "→ Explore" (a relay) or "About Explore"; it opens the lane.
    pub(super) fn relay_chip(
        &self,
        ix: usize,
        r: &crate::agent_conversation::Relay,
        look: &Look,
    ) -> AnyElement {
        let c = look.c;
        let label = self
            .conversation
            .lane(&r.lane)
            .map_or_else(|| "subagent".to_string(), |l| l.label.clone());
        let words = if r.about {
            format!("About {label}")
        } else {
            format!("\u{2192} {label}")
        };
        let tip =
            (!r.about).then(|| format!("fuxx asked Main to pass this to {label}, word for word"));
        let to = View::Lane(r.lane.clone());
        let chip = div()
            .id(("relay-chip", ix))
            .flex_none()
            .self_start()
            .h(px(18.))
            .px(px(6.))
            .flex()
            .items_center()
            .rounded(px(crate::ui_tokens::chat::R_CHIP))
            .bg(hsla(c.chip_fill))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(c.you_dim))
            .cursor_pointer()
            .hover(move |s| s.text_color(hsla(c.you_text)))
            .on_click(on_tab(&look.view, move |this, cx| {
                this.show_view(to.clone(), cx)
            }))
            .child(SharedString::from(words))
            .when_some(tip, |d, tip| {
                d.tooltip(move |_, cx| {
                    cx.new(|_| crate::sidebar_toggle::Tip(tip.clone().into()))
                        .into()
                })
            });
        self.marked("relay-chip", chip).into_any_element()
    }

    /// A relay's caption in its lane (the lanes chore): where it is — held, being passed on,
    /// delivered, not passed on (Send again), or too late (Ask Main instead).
    pub(super) fn relay_caption(
        &self,
        ix: usize,
        item: &Item,
        r: &crate::agent_conversation::Relay,
        look: &Look,
    ) -> AnyElement {
        use crate::agent_conversation::RelayState;
        let c = look.c;
        let label = self
            .conversation
            .lane(&r.lane)
            .map_or_else(|| "subagent".to_string(), |l| l.label.clone());
        let taken = matches!(item, Item::You { taken: true, .. });
        let view = look.view.clone();
        let link = |id: &'static str,
                    words: &'static str,
                    act: fn(&mut AgentTab, usize, &mut Context<AgentTab>)| {
            let view = view.clone();
            div()
                .id((id, ix))
                .cursor_pointer()
                .text_color(hsla(c.text))
                .hover(move |s| s.text_color(hsla(c.accent)))
                .on_click(on_tab(&view, move |this, cx| act(this, ix, cx)))
                .child(words)
                .into_any_element()
        };
        let mut parts: Vec<AnyElement> = vec![
            div()
                .child(SharedString::from(format!("to {label} \u{b7}")))
                .into_any_element(),
        ];
        match (&r.state, taken) {
            (RelayState::Sent, false) => {
                parts.push(
                    div()
                        .child("Main reads it after its step")
                        .into_any_element(),
                );
                if self.cancel_offered(item) {
                    let (dim, ink, hover) = (c.dim, c.text, c.card_hover);
                    parts.push(
                        div()
                            .id(("relay-cancel", ix))
                            .flex_none()
                            .size(px(18.))
                            .rounded(px(crate::ui_tokens::chat::R_CHIP))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_size(px(13.))
                            .text_color(hsla(dim))
                            .hover(move |s| s.bg(hsla(hover)).text_color(hsla(ink)))
                            .tooltip(|_, cx| {
                                cx.new(|_| {
                                    crate::sidebar_toggle::Tip(super::cancel::CANCEL_TIP.into())
                                })
                                .into()
                            })
                            .on_click(on_tab(&view, move |this, cx| this.cancel_message(ix, cx)))
                            .child("\u{d7}")
                            .into_any_element(),
                    );
                }
            }
            (RelayState::Sent, true) => parts.push(
                div()
                    .child("Main is passing it on\u{2026}")
                    .into_any_element(),
            ),
            (RelayState::Delivered, _) => parts.push(
                div()
                    .text_color(hsla(c.ask_done))
                    .child("delivered \u{2713}")
                    .into_any_element(),
            ),
            (RelayState::NotPassed, _) => {
                parts.push(
                    div()
                        .text_color(hsla(c.failed))
                        .child("Main didn't pass this on")
                        .into_any_element(),
                );
                parts.push(link("relay-again", "Send again", |this, ix, cx| {
                    this.relay_again(ix, cx)
                }));
            }
            (RelayState::LaneEnded, _) => {
                parts.push(
                    div()
                        .child(SharedString::from(format!("{label} had finished")))
                        .into_any_element(),
                );
                parts.push(link("relay-instead", "Ask Main instead", |this, ix, cx| {
                    this.ask_instead(ix, cx)
                }));
            }
        }
        self.marked(
            "relay-caption",
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .text_size(px(text::META))
                .text_color(hsla(c.dim))
                .children(parts),
        )
        .into_any_element()
    }

    /// "Send again" on a relay Main didn't pass on: again, if the lane can still be messaged;
    /// else its words go back in the box.
    pub(crate) fn relay_again(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(Item::You {
            text, to: Some(r), ..
        }) = self.conversation.items().get(ix).cloned()
        else {
            return;
        };
        // Again only while this lane's box would relay it now.
        let ready = self.view == View::Lane(r.lane.clone())
            && matches!(
                self.box_mode(cx),
                BoxMode::Relay {
                    ready: true,
                    images: false
                }
            );
        let can = self.conversation.lane(&r.lane).filter(|_| ready).cloned();
        match can {
            Some(lane) => self.send_for_lane(text, Target::of(&lane, false, true), cx),
            None => {
                self.composer.update(cx, |c, cx| c.set_text(text, cx));
                cx.notify();
            }
        }
    }

    /// "Ask Main instead" on a relay its subagent finished before getting: the box opens to
    /// Main about it, your words in it.
    pub(crate) fn ask_instead(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(Item::You {
            text, to: Some(r), ..
        }) = self.conversation.items().get(ix).cloned()
        else {
            return;
        };
        if self.view != View::Lane(r.lane.clone()) {
            self.show_view(View::Lane(r.lane.clone()), cx);
        }
        self.bar_open = Some(r.lane.clone());
        self.draft = Some(Draft {
            lane: r.lane,
            about: true,
        });
        self.composer.update(cx, |c, cx| c.set_text(text, cx));
        self.note_bar(cx);
        self.update_placeholder(cx);
        cx.notify();
    }

    /// "Ask Main about it": the bar becomes the box, empty, in place.
    pub(crate) fn open_bar(&mut self, cx: &mut Context<Self>) {
        if let Some(lane) = self.shown_lane().map(|l| l.id.clone()) {
            self.bar_open = Some(lane);
            self.note_bar(cx);
            self.update_placeholder(cx);
            cx.notify();
        }
    }

    /// ⌫ in an empty box: an opened bar closes again.
    pub(super) fn backspace_empty(&mut self, cx: &mut Context<Self>) {
        if self.bar_open.take().is_some() {
            self.draft = None;
            self.note_bar(cx);
            self.update_placeholder(cx);
            cx.notify();
        }
    }

    /// A draft's lane finished under it (the lanes chore): its target becomes Main, about the
    /// lane; the pill flashes, and an Enter right after only flashes again.
    pub(super) fn sync_draft(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if let Some(d) = &mut self.draft
            && !d.about
            && self
                .conversation
                .lane(&d.lane)
                .is_some_and(|l| !l.status.live())
        {
            d.about = true;
            self.retargeted_at = Some(now);
            self.lane_flash.insert(d.lane.clone(), now);
        }
        self.note_bar(cx);
    }

    /// The box went between the bar and the box: its cross-fade starts.
    pub(super) fn note_bar(&mut self, cx: &App) {
        let bar = matches!(self.box_mode(cx), BoxMode::Bar(_));
        if bar != self.bar_was {
            self.bar_was = bar;
            self.bar_changes += 1;
            self.bar_changed_at = Some(Instant::now());
        }
    }

    /// Whether lane `id`'s pill is lit now (a resume, a relay delivered, a retarget), and the
    /// flash's key; none with reduce motion or Animations off.
    pub(crate) fn lane_flash_key(&self, id: &str, cx: &App) -> Option<u64> {
        if cx.reduce_motion() || !cx.global::<crate::appearance::Appearance>().animations {
            return None;
        }
        let at = self.lane_flash.get(id)?;
        let age = at.elapsed();
        (age < Duration::from_millis(crate::agent_chat::FLASH_MS)).then(|| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (id, at).hash(&mut h);
            h.finish()
        })
    }

    /// Stop lane `id` alone (its agent's own stop: `Action::StopSubagent`).
    pub(crate) fn stop_lane(&mut self, id: &str, cx: &mut Context<Self>) {
        self.write(&Action::StopSubagent {
            lane: id.to_string(),
        });
        cx.notify();
    }

    pub(super) fn prev_lane(&mut self, _: &PrevLane, _: &mut Window, cx: &mut Context<Self>) {
        let to = crate::agent_lanes::step(&self.view, self.conversation.lanes(), -1);
        self.show_view(to, cx);
    }

    pub(super) fn next_lane(&mut self, _: &NextLane, _: &mut Window, cx: &mut Context<Self>) {
        let to = crate::agent_lanes::step(&self.view, self.conversation.lanes(), 1);
        self.show_view(to, cx);
    }

    /// A mark for each live lane (Working or Waiting) and Main's; a lane that went takes its
    /// view back to Main.
    pub(super) fn sync_lane_marks(&mut self, cx: &mut Context<Self>) {
        let lanes = self.conversation.lanes();
        let back_to_main =
            matches!(&self.view, View::Lane(id) if !lanes.iter().any(|l| &l.id == id));
        // Lists of lanes that went (`/clear`, a new conversation) go too.
        self.panes.retain(|view, _| match view {
            View::Main => true,
            View::Lane(id) => lanes.iter().any(|l| &l.id == id),
        });
        let live: Vec<(String, Status)> = lanes
            .iter()
            .filter_map(|l| match l.status {
                LaneStatus::Working => Some((l.id.clone(), Status::Working)),
                LaneStatus::Waiting => Some((l.id.clone(), Status::Waiting)),
                _ => None,
            })
            .collect();
        let empty = lanes.is_empty();
        crate::agent_lanes::stamp_ends(&mut self.lane_times, lanes, Instant::now());
        self.lane_marks
            .retain(|id, _| live.iter().any(|(l, _)| l == id));
        for (id, status) in live {
            let clock = self.clock.clone();
            let mark = self
                .lane_marks
                .entry(id)
                .or_insert_with(|| cx.new(|cx| MarkView::new(&clock, status, px(MARK), BLACK, cx)));
            mark.update(cx, |m, _| m.set(status, px(MARK), BLACK));
        }
        if back_to_main {
            // Its box talks to Main again, and says so.
            self.view = View::Main;
            self.update_placeholder(cx);
        }
        if empty {
            self.main_mark = None;
        } else if self.main_mark.is_none() {
            let clock = self.clock.clone();
            let status = self.status();
            self.main_mark = Some(cx.new(|cx| MarkView::new(&clock, status, px(MARK), BLACK, cx)));
        }
    }

    /// How long lane `id` has run (stopped at its end); none when unknown (a resumed lane).
    pub(crate) fn lane_elapsed(&self, id: &str) -> Option<Duration> {
        let (start, end) = match self.lane_times.get(id) {
            Some(&(start, end)) => (Some(start), end),
            None => (None, None),
        };
        crate::agent_lanes::elapsed(start, end, Instant::now())
    }

    /// A lane's status mark: the live arc, a done check, a failed dot, a stopped ring.
    pub(crate) fn lane_mark(&self, lane: &Lane, c: &Colors) -> AnyElement {
        let dot = |fill: Rgb, hollow: bool| {
            div()
                .size(px(crate::ui_tokens::chat::TOOL_DOT))
                .rounded_full()
                .when(!hollow, |d| d.bg(hsla(fill)))
                .when(hollow, |d| d.border_1().border_color(hsla(fill)))
                .into_any_element()
        };
        let mark = match &lane.status {
            LaneStatus::Working | LaneStatus::Waiting => match self.lane_marks.get(&lane.id) {
                Some(mark) => mark.clone().into_any_element(),
                None => dot(c.todo_active, false),
            },
            LaneStatus::Done => {
                crate::status_mark::done_check(c.tool_ok, px(MARK)).into_any_element()
            }
            LaneStatus::Failed(_) => dot(c.failed_panel, false),
            LaneStatus::Stopped | LaneStatus::Ended => dot(c.panel_dim, true),
        };
        mark_slot(mark)
    }

    /// Main's pill's mark: the agent's own status.
    fn main_pill_mark(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let status = self.status();
        let mark = match (&self.main_mark, status) {
            (Some(mark), Status::Working | Status::Waiting) => {
                mark.update(cx, |m, _| m.set(status, px(MARK), BLACK));
                mark.clone().into_any_element()
            }
            (_, Status::Done) => {
                crate::status_mark::done_check(c.tool_ok, px(MARK)).into_any_element()
            }
            _ => div()
                .size(px(crate::ui_tokens::chat::TOOL_DOT))
                .rounded_full()
                .border_1()
                .border_color(hsla(c.panel_dim))
                .into_any_element(),
        };
        mark_slot(mark)
    }

    /// The pill strip (the Lead UI/UX Designer's spec): under the top bar, in the centred
    /// column, once a subagent exists — Main first, then each lane in start order; the shown one
    /// filled; one waiting on you tinted; the ones that don't fit behind "+N".
    pub(super) fn lane_strip(&self, look: &Look, cx: &mut Context<Self>) -> Option<AnyElement> {
        let lanes = self.conversation.lanes();
        if lanes.is_empty() {
            return None;
        }
        let c = look.c;
        let width = self.bar_width.get();
        let max = if width < NARROW { 12 } else { 20 };
        let labels = crate::agent_lanes::pill_labels(lanes, max);
        // Pill widths, estimated: a mark, the label in the meta size, the padding.
        // (7 a character: the labels are MEDIUM; 6.6 ran short once "+N" grew a mark.)
        let pill_w = |label: &str| MARK + 5.0 + label.chars().count() as f32 * 7.0 + 16.0;
        // Main, then the lanes newest first: strip place `n` (1…) is lane `order[n - 1]`.
        let order = crate::agent_lanes::shown_order(lanes);
        let mut widths = vec![pill_w("Main")];
        widths.extend(order.iter().map(|&i| pill_w(&labels[i])));
        let avail = (width - 2.0 * AREA_X).min(crate::ui_tokens::chat::COLUMN_MAX);
        let active = match &self.view {
            View::Main => 0,
            View::Lane(id) => order
                .iter()
                .position(|&i| &lanes[i].id == id)
                .map_or(0, |p| p + 1),
        };
        // "+N" carries a live mark while any lane runs (the lanes chore): room for it.
        let more_w = if lanes.iter().any(|l| l.status.live()) {
            // Its padding, the mark and its gap, and up to "+99".
            16.0 + MARK + 5.0 + 3.0 * 7.0
        } else {
            crate::agent_lanes::MORE_W
        };
        let (shown, hidden) = crate::agent_lanes::fit_with(&widths, avail, active, more_w);
        let neutral = mix(c.page, c.text, 0.06);
        let view = look.view.clone();
        let relays = self.provider.is_some_and(|p| p.relays_to_subagents());
        let pill = |n: usize,
                    mark: AnyElement,
                    label: String,
                    to: View,
                    waiting: bool,
                    tip: String,
                    flash: Option<u64>| {
            let on = n == active;
            let rest = if waiting {
                Some(c.ask_fill)
            } else if on {
                Some(neutral)
            } else {
                None
            };
            let el = div()
                .id(("lane-pill", n))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(5.))
                .h(px(crate::card_view::PILL_H))
                .px(px(8.))
                .rounded(px(6.))
                .text_size(px(text::META))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(if on { c.text } else { c.dim }))
                .when(waiting, |d| d.bg(hsla(c.ask_fill)))
                .when(on && !waiting, |d| d.bg(hsla(neutral)))
                .when(!on, |d| {
                    d.cursor_pointer().hover(move |s| s.bg(hsla(c.panel_hover)))
                })
                .on_click(on_tab(&view, move |this, cx| {
                    this.show_view(to.clone(), cx)
                }))
                .child(mark)
                .child(SharedString::from(label));
            let el = if tip.is_empty() {
                el
            } else {
                el.tooltip(move |_, cx| -> gpui::AnyView {
                    cx.new(|_| crate::sidebar_toggle::Tip(tip.clone().into()))
                        .into()
                })
            };
            // One flash (the lanes chore: resumed, a relay delivered, a draft retargeted).
            match flash {
                Some(key) => {
                    let (base, accent) = (rest.unwrap_or(c.page), c.accent);
                    el.with_animation(
                        ("lane-pill-flash", key),
                        Animation::new(Duration::from_millis(crate::agent_chat::FLASH_MS)),
                        move |d, t| {
                            let k = crate::agent_chat::flash_strength(t);
                            d.bg(hsla(mix(base, accent, 0.30 * k)))
                        },
                    )
                    .into_any_element()
                }
                None => el.into_any_element(),
            }
        };
        let mut pills: Vec<AnyElement> = Vec::new();
        for &n in &shown {
            if n == 0 {
                let mark = self.main_pill_mark(&c, cx);
                pills.push(pill(
                    0,
                    mark,
                    "Main".into(),
                    View::Main,
                    false,
                    String::new(),
                    None,
                ));
                continue;
            }
            let lane = &lanes[order[n - 1]];
            pills.push(pill(
                n,
                self.lane_mark(lane, &c),
                labels[order[n - 1]].clone(),
                View::Lane(lane.id.clone()),
                lane.status == LaneStatus::Waiting,
                crate::agent_lanes::pill_tip(lane, self.lane_elapsed(&lane.id), relays),
                self.lane_flash_key(&lane.id, cx),
            ));
        }
        let more = (hidden > 0).then(|| {
            let hidden_lanes: Vec<usize> = (1..widths.len())
                .filter(|n| !shown.contains(n))
                .map(|n| order[n - 1])
                .collect();
            let menu = self.lanes_menu.then(|| {
                let (live, done) =
                    crate::agent_lanes::list_groups(lanes, &hidden_lanes, &self.lanes_menu_running);
                let both = !live.is_empty() && !done.is_empty();
                let row = |i: usize| {
                    let lane = &lanes[i];
                    let to = View::Lane(lane.id.clone());
                    let time = self.lane_elapsed(&lane.id).map(crate::agent_lanes::clock);
                    div()
                        .id(("lane-menu-row", i))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(8.))
                        .h(px(26.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hsla(c.panel_hover)))
                        .on_click(on_tab(&view, move |this, cx| {
                            this.show_view(to.clone(), cx)
                        }))
                        .child(self.lane_mark(lane, &c))
                        .child(div().flex_1().child(SharedString::from(labels[i].clone())))
                        .children(time.map(|t| {
                            div()
                                .text_color(hsla(c.panel_dim))
                                .child(SharedString::from(t))
                        }))
                        .into_any_element()
                };
                let header = |words: &'static str| {
                    div()
                        .h(px(22.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .text_size(px(text::GROUP_HEADER))
                        .text_color(hsla(c.panel_dim))
                        .child(words)
                        .into_any_element()
                };
                let mut rows: Vec<AnyElement> = Vec::new();
                if both {
                    rows.push(header("Running"));
                }
                rows.extend(live.iter().map(|&i| row(i)));
                if both {
                    rows.push(header("Finished"));
                }
                rows.extend(done.iter().map(|&i| row(i)));
                gpui::deferred(
                    gpui::anchored().snap_to_window_with_margin(px(8.)).child(
                        div()
                            .mt(px(4.))
                            .min_w(px(200.))
                            .p(px(4.))
                            .rounded(px(8.))
                            .bg(hsla(c.panel))
                            .border_1()
                            .border_color(hsla(c.panel_border))
                            .text_size(px(text::META))
                            .text_color(hsla(c.panel_text))
                            .occlude()
                            .child(self.marked("lanes-menu", div().children(rows))),
                    ),
                )
            });
            // A live mark when a lane behind it runs, tinted when one waits on you.
            let running: Vec<&Lane> = hidden_lanes
                .iter()
                .map(|&i| &lanes[i])
                .filter(|l| l.status.live())
                .collect();
            let waits = running.iter().any(|l| l.status == LaneStatus::Waiting);
            let live_mark = running
                .iter()
                .find(|l| l.status == LaneStatus::Waiting)
                .or(running.first())
                .map(|l| self.lane_mark(l, &c));
            let tip = crate::agent_lanes::more_tip(hidden, running.len());
            div()
                .flex_none()
                .child(
                    div()
                        .id("lane-more")
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .h(px(crate::card_view::PILL_H))
                        .px(px(8.))
                        .rounded(px(6.))
                        .text_size(px(text::META))
                        .text_color(hsla(c.dim))
                        .when(waits, |d| d.bg(hsla(c.ask_fill)))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hsla(c.panel_hover)))
                        .on_click(on_tab(&view, |this, cx| this.toggle_lanes_menu(cx)))
                        .tooltip(move |_, cx| -> gpui::AnyView {
                            cx.new(|_| crate::sidebar_toggle::Tip(tip.clone().into()))
                                .into()
                        })
                        .children(live_mark)
                        .child(SharedString::from(format!("+{hidden}"))),
                )
                .children(menu)
        });
        let more = more.map(|m| self.marked("more", m));
        Some(
            div()
                .flex_none()
                .w_full()
                .h(px(STRIP_H))
                .px(px(AREA_X))
                .flex()
                .justify_center()
                .child(
                    self.marked(
                        "strip",
                        div()
                            .w_full()
                            .max_w(px(crate::ui_tokens::chat::COLUMN_MAX))
                            .flex()
                            .items_center()
                            .gap(px(crate::agent_lanes::PILL_GAP))
                            .children(pills)
                            .children(more),
                    ),
                )
                .into_any_element(),
        )
    }

    /// The shown lane, if the view is one.
    pub(crate) fn shown_lane(&self) -> Option<&Lane> {
        match &self.view {
            View::Lane(id) => self.conversation.lane(id),
            View::Main => None,
        }
    }

    /// Back to Main, its list brought to the lane's row ("Back to Main ›").
    pub(crate) fn back_to_main(&mut self, lane: &str, cx: &mut Context<Self>) {
        self.show_view(View::Main, cx);
        let items = self.conversation.items();
        let pane = self.pane();
        if let Some(k) = pane
            .order
            .iter()
            .position(|&i| matches!(&items[i], Item::Tool { id, .. } if id == lane))
        {
            pane.list.scroll_to_reveal_item(k);
        }
    }

    /// A lane's header (the Lead UI/UX Designer's spec), over its list: its name, its kind and
    /// model, and on the right its time, tokens and tools as far as the agent says (a narrow tab
    /// keeps the time).
    pub(super) fn lane_header(&self, look: &Look, _cx: &mut Context<Self>) -> Option<AnyElement> {
        let lane = self.shown_lane()?;
        let c = look.c;
        let narrow = self.bar_width.get() < NARROW;
        let kind = [Some(lane.kind.clone()), lane.model.clone()]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let stats = [
            self.lane_elapsed(&lane.id).map(crate::agent_lanes::clock),
            lane.tokens
                .filter(|_| !narrow)
                .map(|t| format!("{:.1}k tok", t as f32 / 1000.0)),
            lane.tools.filter(|_| !narrow).map(|n| match n {
                1 => "1 tool".to_string(),
                n => format!("{n} tools"),
            }),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let first = self
            .lane_runs
            .get(&lane.id)
            .and_then(|runs| runs.first().copied().flatten());
        let run_tip = match (lane.runs > 1, self.lane_elapsed(&lane.id), first) {
            (true, Some(now), Some(first)) => Some(format!(
                "This run {} \u{b7} first run {}",
                crate::agent_lanes::clock(now),
                crate::agent_lanes::clock(first)
            )),
            _ => None,
        };
        Some(
            div()
                .flex_none()
                .w_full()
                .px(px(AREA_X))
                .pb(px(4.))
                .flex()
                .justify_center()
                .child(
                    self.marked(
                        "lane-header",
                        div()
                            .w_full()
                            .max_w(px(crate::ui_tokens::chat::COLUMN_MAX))
                            .h(px(crate::card_view::PILL_H))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(text::META))
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(hsla(c.text))
                                    .child(SharedString::from(lane.label.clone())),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .truncate()
                                    .text_color(hsla(c.dim))
                                    .child(SharedString::from(kind)),
                            )
                            .child(
                                div()
                                    .id("lane-stats")
                                    .flex_none()
                                    .font_features(crate::sidebar::tabular())
                                    .text_color(hsla(c.panel_dim))
                                    .child(SharedString::from(stats))
                                    // Resumed (the lanes chore): the time is this run's.
                                    .when_some(run_tip, |d, tip| {
                                        d.tooltip(move |_, cx| {
                                            cx.new(|_| {
                                                crate::sidebar_toggle::Tip(tip.clone().into())
                                            })
                                            .into()
                                        })
                                    }),
                            )
                            .when(crate::agent_lanes::shows_stop(lane), |d| {
                                let view = look.view.clone();
                                let id = lane.id.clone();
                                d.child(self.marked(
                                    "lane-stop",
                                    div().child(button("lane-stop", "Stop", false, &c).on_click(
                                        on_tab(&view, move |this, cx| this.stop_lane(&id, cx)),
                                    )),
                                ))
                            }),
                    ),
                )
                .into_any_element(),
        )
    }

    /// A lane's first row: what Main asked it, as your bubble is drawn, captioned "from Main".
    pub(super) fn lane_head_row(&self, look: &Look) -> AnyElement {
        let Some(lane) = self.shown_lane() else {
            return div().into_any_element();
        };
        let c = look.c;
        let task = lane.task.trim().to_string();
        div()
            .w_full()
            .px(px(AREA_X))
            .pt(px(crate::ui_tokens::chat::FIRST_TOP))
            .flex()
            .flex_col()
            .items_center()
            .child(
                self.marked(
                    "lane-task",
                    div()
                        .w_full()
                        .max_w(px(crate::ui_tokens::chat::COLUMN_MAX))
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(px(4.))
                        .child(
                            div()
                                .text_size(px(text::META))
                                .text_color(hsla(c.dim))
                                .child("from Main"),
                        )
                        // Lit a moment when the strip jumps to it (chore after M11ar).
                        .child(
                            self.flashed(
                                div()
                                    .max_w(px(520.))
                                    .px(px(14.))
                                    .py(px(9.))
                                    .rounded(px(crate::ui_tokens::chat::R_BUBBLE))
                                    .rounded_tr(px(crate::ui_tokens::chat::R_TAIL))
                                    .bg(hsla(c.you_fill))
                                    .border_1()
                                    .border_color(hsla(c.you_edge))
                                    .text_size(px(crate::ui_tokens::chat::BODY))
                                    .line_height(relative(crate::ui_tokens::chat::BODY_LINE))
                                    .text_color(hsla(c.you_text))
                                    .child(SharedString::from(task)),
                                0,
                                &c,
                            ),
                        ),
                ),
            )
            .into_any_element()
    }

    /// A lane's last row once it ended: "✓ Done in 1:42 · 18.2k tokens", "✕ Failed · why",
    /// "Stopped", "Ended without a report" (the closing bar under it owns Back to Main since
    /// the lanes chore).
    pub(super) fn lane_end_row(&self, look: &Look) -> AnyElement {
        let Some(lane) = self.shown_lane() else {
            return div().into_any_element();
        };
        let c = look.c;
        let (word, ink): (String, Rgb) = match &lane.status {
            // "✓ Done in 1:42 · 18.2k tokens", each part when known.
            LaneStatus::Done => (
                [
                    Some(match self.lane_elapsed(&lane.id) {
                        Some(d) => format!("\u{2713} Done in {}", crate::agent_lanes::clock(d)),
                        None => "\u{2713} Done".into(),
                    }),
                    lane.tokens
                        .map(|t| format!("{:.1}k tokens", t as f32 / 1000.0)),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" \u{b7} "),
                c.tool_ok,
            ),
            LaneStatus::Failed(why) => (format!("\u{2715} Failed \u{b7} {why}"), c.failed_panel),
            LaneStatus::Ended => ("Ended without a report".into(), c.panel_dim),
            _ => ("Stopped".into(), c.panel_dim),
        };
        div()
            .w_full()
            .px(px(AREA_X))
            .pt(px(crate::ui_tokens::chat::GAP_NOTICE))
            .pb(px(crate::ui_tokens::chat::LAST_BOTTOM))
            .flex()
            .flex_col()
            .items_center()
            .child(
                self.marked(
                    "lane-end",
                    div()
                        .w_full()
                        .max_w(px(crate::ui_tokens::chat::COLUMN_MAX))
                        .pl(px(
                            crate::ui_tokens::chat::AVATAR + crate::ui_tokens::chat::AVATAR_GAP
                        ))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(text::META))
                        .child(div().text_color(hsla(ink)).child(SharedString::from(word))),
                ),
            )
            .into_any_element()
    }

    /// A resume's row in its lane (the lanes chore): the last run's end as a divider between
    /// hairlines, "✓ Done in 1:33 · resumed by Main", then what Main said to it ("from Main").
    pub(super) fn resumed_row(
        &self,
        lane: &str,
        ended: &LaneStatus,
        said: Option<&str>,
        run: u32,
        look: &Look,
    ) -> AnyElement {
        let c = look.c;
        let took = self
            .lane_runs
            .get(lane)
            .and_then(|runs| runs.get(run.saturating_sub(2) as usize))
            .copied()
            .flatten();
        let mark = match ended {
            LaneStatus::Done => "\u{2713} ",
            LaneStatus::Failed(_) => "\u{2715} ",
            _ => "",
        };
        let words = format!("{mark}{} \u{b7} resumed by Main", ended_words(ended, took));
        let hairline = || div().flex_1().h(px(1.)).bg(hsla(c.tool_divider));
        let divider = (!ended.live()).then(|| {
            self.marked(
                "lane-resumed",
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(px(text::META))
                    .text_color(hsla(c.dim))
                    .child(hairline())
                    .child(div().flex_none().child(SharedString::from(words)))
                    .child(hairline()),
            )
        });
        let said = said.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(crate::ui_tokens::chat::GAP_ITEM))
            .children(divider)
            .children(said.map(|t| from_main(t, &c)))
            .into_any_element()
    }

    /// Scenes: the lanes, and a lane's step count.
    #[cfg(feature = "ui-shots")]
    pub fn shot_lanes(&self) -> &[Lane] {
        self.conversation.lanes()
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_lane_steps(&self, id: &str) -> usize {
        self.conversation.lane_steps(id)
    }

    /// Scenes: whether `part` has been drawn (since `shot_forget`).
    #[cfg(feature = "ui-shots")]
    pub fn shot_part_now(&self, part: &str) -> bool {
        self.shot_part(part).is_some()
    }

    /// Scenes: forget where `parts` were drawn (the next frame says again).
    #[cfg(feature = "ui-shots")]
    pub fn shot_forget(&self, parts: &[&str]) {
        self.parts.borrow_mut().retain(|k, _| !parts.contains(k));
    }

    /// Scenes: a followed file's lines for `lane`, into the same path the follower takes.
    #[cfg(feature = "ui-shots")]
    pub fn shot_followed(&mut self, lane: &str, lines: Vec<String>, cx: &mut Context<Self>) {
        self.on_followed(lane, lines, cx);
    }

    /// Scenes: open or close the "+N" list.
    #[cfg(feature = "ui-shots")]
    pub fn shot_toggle_lanes_menu(&mut self, cx: &mut Context<Self>) {
        self.toggle_lanes_menu(cx);
    }

    /// Open or close the "+N" list; opening it takes which lanes run, so a lane finishing
    /// while it is open keeps its row (the lanes chore: it regroups on the next open).
    pub(crate) fn toggle_lanes_menu(&mut self, cx: &mut Context<Self>) {
        self.lanes_menu = !self.lanes_menu;
        if self.lanes_menu {
            self.lanes_menu_running = self
                .conversation
                .lanes()
                .iter()
                .filter(|l| l.status.live())
                .map(|l| l.id.clone())
                .collect();
        }
        cx.notify();
    }
}

/// What the box is now (the lanes chore; `AgentTab::box_mode`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BoxMode {
    /// To Main: Main's view, or an open question or permission.
    Main,
    /// In a lane of an agent whose Main can't pass a message on (agy): still to Main.
    MainFromLane,
    /// To the lane's subagent via Main; its send off with `images`, or not `ready`.
    Relay { images: bool, ready: bool },
    /// To Main about the lane (finished, or `live` inside Main's step).
    About { live: bool },
    /// The closing bar.
    Bar(Bar),
}

/// The subagent a message of yours is for (the lanes chore), as it waits and as it is sent.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Target {
    pub lane: String,
    pub label: String,
    pub about: bool,
    pub live: bool,
}

impl Target {
    fn of(lane: &Lane, about: bool, live: bool) -> Target {
        Target {
            lane: lane.id.clone(),
            label: lane.label.clone(),
            about,
            live,
        }
    }

    /// The action that sends `text` (`id` the message's) for it.
    pub(crate) fn action(&self, text: String, id: Option<String>) -> Action {
        let (lane, label) = (self.lane.clone(), self.label.clone());
        if self.about {
            Action::About {
                lane,
                label,
                text,
                id,
                live: self.live,
            }
        } else {
            Action::Relay {
                lane,
                label,
                text,
                id,
            }
        }
    }
}

/// What the box's draft in a lane is for (the lanes chore): set when it stops being empty, so a
/// lane finishing under your fingers never swaps the box.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Draft {
    pub lane: String,
    /// To Main about the lane (else a relay to it).
    pub about: bool,
}

/// How a lane's run ended, as its divider and the closing bar say it: "Done in 1:33", "Failed",
/// "Stopped after 0:48", "Ended without a report".
pub(crate) fn ended_words(ended: &LaneStatus, took: Option<Duration>) -> String {
    let time = took.map(crate::agent_lanes::clock);
    match (ended, time) {
        (LaneStatus::Done, Some(t)) => format!("Done in {t}"),
        (LaneStatus::Done, None) => "Done".into(),
        (LaneStatus::Failed(_), _) => "Failed".into(),
        (LaneStatus::Stopped, Some(t)) => format!("Stopped after {t}"),
        (LaneStatus::Stopped, None) => "Stopped".into(),
        (LaneStatus::Ended, _) => "Ended without a report".into(),
        (LaneStatus::Working | LaneStatus::Waiting, _) => "Running".into(),
    }
}

/// What Main said to a lane, as your bubble is drawn, captioned "from Main" (its first row, a
/// resume's).
fn from_main(text: String, c: &Colors) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_end()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(text::META))
                .text_color(hsla(c.dim))
                .child("from Main"),
        )
        .child(
            div()
                .max_w(px(520.))
                .px(px(14.))
                .py(px(9.))
                .rounded(px(crate::ui_tokens::chat::R_BUBBLE))
                .rounded_tr(px(crate::ui_tokens::chat::R_TAIL))
                .bg(hsla(c.you_fill))
                .border_1()
                .border_color(hsla(c.you_edge))
                .text_size(px(crate::ui_tokens::chat::BODY))
                .line_height(relative(crate::ui_tokens::chat::BODY_LINE))
                .text_color(hsla(c.you_text))
                .child(SharedString::from(text)),
        )
}

/// A mark in the tool row's 12 pt slot.
fn mark_slot(mark: AnyElement) -> AnyElement {
    div()
        .flex_none()
        .size(px(MARK))
        .flex()
        .items_center()
        .justify_center()
        .child(mark)
        .into_any_element()
}
